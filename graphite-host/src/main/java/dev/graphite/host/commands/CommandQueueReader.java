package dev.graphite.host.commands;

import dev.graphite.host.SharedMemory;
import net.minecraft.core.BlockPos;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;

public class CommandQueueReader {
    private static final Logger LOG = LoggerFactory.getLogger(CommandQueueReader.class);
    private static final int MAX_COMMANDS_PER_TICK = 1024;
    private static final int CQ_CAPACITY = 0x7FF0 - CommandType.CQ_DATA_OFFSET;

    private final ByteBuffer stateBuffer;
    private final int cqBase;
    private final BlockPos.MutableBlockPos mutablePos = new BlockPos.MutableBlockPos();

    public CommandQueueReader(ByteBuffer stateBuffer) {
        this.stateBuffer = stateBuffer.duplicate().order(ByteOrder.LITTLE_ENDIAN);
        this.cqBase = SharedMemory.OFFSET_COMMAND_QUEUE;
    }

    public void drain(ServerLevel level) {
        int head = SharedMemory.getIntAcquire(stateBuffer, cqBase + CommandType.CQ_HEAD_OFFSET);
        int tail = SharedMemory.getIntAcquire(stateBuffer, cqBase + CommandType.CQ_TAIL_OFFSET);

        if (head == tail) {
            return;
        }

        int dataBase = cqBase + CommandType.CQ_DATA_OFFSET;
        int processed = 0;

        while (head != tail && processed < MAX_COMMANDS_PER_TICK) {
            byte cmdType = readQueueByte(dataBase, head);
            int payloadLen = readQueueByte(dataBase, head + 1) & 0xFF;
            int payloadOff = head + 2;
            int totalSize = 2 + payloadLen;

            try {
                dispatch(level, cmdType, dataBase, payloadOff, payloadLen);
            } catch (Exception e) {
                LOG.error(String.format("[Graphite] Command 0x%02X failed", cmdType), e);
            }

            head += totalSize;
            processed++;
        }

        if (processed == MAX_COMMANDS_PER_TICK && head != tail) {
            LOG.warn("[Graphite] Command limit reached, dropping remaining commands");
            head = tail;
        }

        SharedMemory.setIntRelease(stateBuffer, cqBase + CommandType.CQ_HEAD_OFFSET, head);
    }

    private void dispatch(ServerLevel level, byte type, int dataBase, int off, int len) {
        switch (type) {
            case CommandType.SET_BLOCK -> execSetBlock(level, dataBase, off, len);
            case CommandType.SEND_CHAT -> execSendChat(level, dataBase, off, len);
            case CommandType.SET_VELOCITY -> execSetVelocity(level, dataBase, off, len);
            case CommandType.KILL_ENTITY -> execKillEntity(level, dataBase, off, len);
            case CommandType.SPAWN_PARTICLE -> execSpawnParticle(level, dataBase, off, len);
            default -> LOG.warn(String.format("[Graphite] Unknown command type 0x%02X", type));
        }
    }

    private void execSetBlock(ServerLevel level, int base, int off, int len) {
        if (len != 16) {
            LOG.warn("[Graphite] SET_BLOCK invalid payload length={}", len);
            return;
        }

        int x = readQueueI32(base, off);
        int y = readQueueI32(base, off + 4);
        int z = readQueueI32(base, off + 8);
        int stateId = readQueueI32(base, off + 12);

        BlockState state = Block.stateById(stateId);
        if (state == null) {
            LOG.warn("[Graphite] SET_BLOCK invalid stateId={}", stateId);
            return;
        }

        mutablePos.set(x, y, z);
        level.setBlock(mutablePos, state, Block.UPDATE_ALL);
    }

    private void execSendChat(ServerLevel level, int base, int off, int len) {
        if (len < 5) {
            LOG.warn("[Graphite] SEND_CHAT invalid payload length={}", len);
            return;
        }

        int playerId = readQueueI32(base, off);
        int msgLen = readQueueByte(base, off + 4) & 0xFF;
        if (msgLen != len - 5) {
            LOG.warn("[Graphite] SEND_CHAT declared message length {} does not match payload {}", msgLen, len);
            return;
        }
        byte[] msgBytes = new byte[msgLen];
        for (int index = 0; index < msgLen; index++) {
            msgBytes[index] = readQueueByte(base, off + 5 + index);
        }

        String message = new String(msgBytes, StandardCharsets.UTF_8);
        for (ServerPlayer player : level.players()) {
            if (player.getId() == playerId || playerId == -1) {
                player.sendSystemMessage(Component.literal(message));
            }
        }
    }

    private void execSetVelocity(ServerLevel level, int base, int off, int len) {
        if (len != 16) {
            LOG.warn("[Graphite] SET_VELOCITY invalid payload length={}", len);
            return;
        }

        int entityId = readQueueI32(base, off);
        float vx = readQueueF32(base, off + 4);
        float vy = readQueueF32(base, off + 8);
        float vz = readQueueF32(base, off + 12);

        Entity entity = level.getEntity(entityId);
        if (entity != null) {
            entity.setDeltaMovement(new Vec3(vx, vy, vz));
            if (entity instanceof ServerPlayer player) {
                player.connection.send(new ClientboundSetEntityMotionPacket(entity));
            }
        }
    }

    private void execKillEntity(ServerLevel level, int base, int off, int len) {
        if (len != 4) {
            LOG.warn("[Graphite] KILL_ENTITY invalid payload length={}", len);
            return;
        }

        int entityId = readQueueI32(base, off);
        Entity entity = level.getEntity(entityId);
        if (entity != null) {
            entity.discard();
        }
    }

    private void execSpawnParticle(ServerLevel level, int base, int off, int len) {
        if (len != 32) {
            LOG.warn("[Graphite] SPAWN_PARTICLE invalid payload length={}", len);
            return;
        }

        int particleId = readQueueI32(base, off);
        double x = readQueueF64(base, off + 4);
        double y = readQueueF64(base, off + 12);
        double z = readQueueF64(base, off + 20);
        float count = readQueueF32(base, off + 28);
        level.sendParticles(resolveParticle(particleId), x, y, z, (int) count, 0.1, 0.1, 0.1, 0.01);
    }

    private byte readQueueByte(int dataBase, int relativeOff) {
        return stateBuffer.get(dataBase + Math.floorMod(relativeOff, CQ_CAPACITY));
    }

    private int readQueueI32(int dataBase, int relativeOff) {
        byte[] bytes = new byte[4];
        for (int i = 0; i < 4; i++) {
            bytes[i] = readQueueByte(dataBase, relativeOff + i);
        }
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getInt();
    }

    private float readQueueF32(int dataBase, int relativeOff) {
        byte[] bytes = new byte[4];
        for (int i = 0; i < 4; i++) {
            bytes[i] = readQueueByte(dataBase, relativeOff + i);
        }
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getFloat();
    }

    private double readQueueF64(int dataBase, int relativeOff) {
        byte[] bytes = new byte[8];
        for (int i = 0; i < 8; i++) {
            bytes[i] = readQueueByte(dataBase, relativeOff + i);
        }
        return ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getDouble();
    }

    private ParticleOptions resolveParticle(int particleId) {
        return switch (particleId) {
            case 1 -> ParticleTypes.HEART;
            case 2 -> ParticleTypes.CRIT;
            case 3 -> ParticleTypes.END_ROD;
            case 4 -> ParticleTypes.ENCHANT;
            case 5 -> ParticleTypes.SMOKE;
            default -> ParticleTypes.FLAME;
        };
    }
}
