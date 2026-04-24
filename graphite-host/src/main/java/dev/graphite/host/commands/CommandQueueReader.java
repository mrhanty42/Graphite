package dev.graphite.host.commands;

import dev.graphite.host.SharedMemory;
import net.minecraft.core.BlockPos;
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
            byte cmdType = readByte(dataBase + head % CQ_CAPACITY);
            int payloadLen = readByte(dataBase + (head + 1) % CQ_CAPACITY) & 0xFF;
            int payloadOff = (head + 2) % CQ_CAPACITY;
            int totalSize = 2 + payloadLen;

            try {
                dispatch(level, cmdType, dataBase, payloadOff, payloadLen);
            } catch (Exception e) {
                LOG.error(String.format("[Graphite] Command 0x%02X failed", cmdType), e);
            }

            head = (head + totalSize) % CQ_CAPACITY;
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
            case CommandType.SET_BLOCK -> execSetBlock(level, dataBase, off);
            case CommandType.SEND_CHAT -> execSendChat(level, dataBase, off, len);
            case CommandType.SET_VELOCITY -> execSetVelocity(level, dataBase, off);
            case CommandType.KILL_ENTITY -> execKillEntity(level, dataBase, off);
            case CommandType.SPAWN_PARTICLE -> execSpawnParticle(level, dataBase, off);
            default -> LOG.warn(String.format("[Graphite] Unknown command type 0x%02X", type));
        }
    }

    private void execSetBlock(ServerLevel level, int base, int off) {
        int x = readI32(base + off % CQ_CAPACITY);
        int y = readI32(base + (off + 4) % CQ_CAPACITY);
        int z = readI32(base + (off + 8) % CQ_CAPACITY);
        int stateId = readI32(base + (off + 12) % CQ_CAPACITY);

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
            return;
        }

        int playerId = readI32(base + off % CQ_CAPACITY);
        int msgLen = readByte(base + (off + 4) % CQ_CAPACITY) & 0xFF;
        byte[] msgBytes = new byte[msgLen];
        for (int index = 0; index < msgLen; index++) {
            msgBytes[index] = readByte(base + (off + 5 + index) % CQ_CAPACITY);
        }

        String message = new String(msgBytes, StandardCharsets.UTF_8);
        for (ServerPlayer player : level.players()) {
            if (player.getId() == playerId || playerId == -1) {
                player.sendSystemMessage(Component.literal(message));
            }
        }
    }

    private void execSetVelocity(ServerLevel level, int base, int off) {
        int entityId = readI32(base + off % CQ_CAPACITY);
        float vx = readF32(base + (off + 4) % CQ_CAPACITY);
        float vy = readF32(base + (off + 8) % CQ_CAPACITY);
        float vz = readF32(base + (off + 12) % CQ_CAPACITY);

        Entity entity = level.getEntity(entityId);
        if (entity != null) {
            entity.setDeltaMovement(new Vec3(vx, vy, vz));
            if (entity instanceof ServerPlayer player) {
                player.connection.send(new ClientboundSetEntityMotionPacket(entity));
            }
        }
    }

    private void execKillEntity(ServerLevel level, int base, int off) {
        int entityId = readI32(base + off % CQ_CAPACITY);
        Entity entity = level.getEntity(entityId);
        if (entity != null) {
            entity.discard();
        }
    }

    private void execSpawnParticle(ServerLevel level, int base, int off) {
        double x = readF64(base + (off + 4) % CQ_CAPACITY);
        double y = readF64(base + (off + 12) % CQ_CAPACITY);
        double z = readF64(base + (off + 20) % CQ_CAPACITY);
        float count = readF32(base + (off + 28) % CQ_CAPACITY);
        level.sendParticles(ParticleTypes.FLAME, x, y, z, (int) count, 0.1, 0.1, 0.1, 0.01);
    }

    private byte readByte(int absOff) {
        return stateBuffer.get(absOff);
    }

    private int readI32(int absOff) {
        return stateBuffer.getInt(absOff);
    }

    private float readF32(int absOff) {
        return stateBuffer.getFloat(absOff);
    }

    private double readF64(int absOff) {
        return stateBuffer.getDouble(absOff);
    }

}
