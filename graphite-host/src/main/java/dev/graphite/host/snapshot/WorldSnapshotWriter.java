package dev.graphite.host.snapshot;

import dev.graphite.host.SharedMemory;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

public class WorldSnapshotWriter {
    private static final int SNAPSHOT_LIMIT = SharedMemory.OFFSET_COMMAND_QUEUE;
    private static final int PROTOCOL_VERSION = 2;
    private static final int SNAP_ENTITY_COUNT = 0;
    private static final int SNAP_CHUNK_SEC_COUNT = 4;
    private static final int SNAP_TIMESTAMP_NS = 8;
    private static final int SNAP_VERSION = 16;
    private static final int SNAP_FLAGS = 20;
    private static final int SNAP_ENTITY_DATA_SIZE = 24;
    private static final int SNAP_CHUNK_DATA_SIZE = 28;
    private static final int SNAP_HEADER_SIZE = 32;

    private static final int ENTITY_RECORD_SIZE = 48;
    private static final int CHUNK_SECTION_RECORD_SIZE = 16400;
    private static final int MAX_ENTITIES = 4096;
    private static final int MAX_CHUNK_SECTIONS = 24;

    private static final short FLAG_ALIVE = 1;
    private static final short FLAG_ON_GROUND = 2;
    private static final short FLAG_IN_WATER = 4;
    private static final short FLAG_SPRINTING = 8;
    private static final int CHUNK_RADIUS = 1;

    private final ByteBuffer stateBuffer;
    private final int snapshotBase;

    public WorldSnapshotWriter(ByteBuffer stateBuffer) {
        this.stateBuffer = stateBuffer.duplicate().order(ByteOrder.LITTLE_ENDIAN);
        this.snapshotBase = SharedMemory.OFFSET_WORLD_SNAPSHOT;
    }

    public void write(ServerLevel level, long tickId) {
        int entityDataStart = snapshotBase + SNAP_HEADER_SIZE;
        int entityCount = writeEntities(level, entityDataStart);

        int chunkDataStart = entityDataStart + entityCount * ENTITY_RECORD_SIZE;
        int maxChunkSlots = Math.max(0, (SNAPSHOT_LIMIT - chunkDataStart) / CHUNK_SECTION_RECORD_SIZE);
        int chunkCount = writeChunkSections(level, chunkDataStart, Math.min(MAX_CHUNK_SECTIONS, maxChunkSlots));

        // Write header last so Rust never sees a partially-written snapshot
        stateBuffer.putInt(snapshotBase + SNAP_ENTITY_COUNT, entityCount);
        stateBuffer.putInt(snapshotBase + SNAP_CHUNK_SEC_COUNT, chunkCount);
        stateBuffer.putLong(snapshotBase + SNAP_TIMESTAMP_NS, System.nanoTime());
        stateBuffer.putInt(snapshotBase + SNAP_VERSION, PROTOCOL_VERSION);
        stateBuffer.putInt(snapshotBase + SNAP_FLAGS, 0);
        stateBuffer.putInt(snapshotBase + SNAP_ENTITY_DATA_SIZE, entityCount * ENTITY_RECORD_SIZE);
        stateBuffer.putInt(snapshotBase + SNAP_CHUNK_DATA_SIZE, chunkCount * CHUNK_SECTION_RECORD_SIZE);

        // Release stores ensure Rust sees the snapshot data before the ready flag
        SharedMemory.setIntRelease(stateBuffer, SharedMemory.OFFSET_COMMAND_COUNT, 0);
        SharedMemory.setLongRelease(stateBuffer, SharedMemory.OFFSET_TICK_COUNTER, tickId);
        SharedMemory.setIntRelease(stateBuffer, SharedMemory.OFFSET_SNAPSHOT_READY, 1);
    }

    private int writeEntities(ServerLevel level, int baseOffset) {
        int count = 0;

        for (Entity entity : level.getAllEntities()) {
            if (count >= MAX_ENTITIES) {
                break;
            }

            int off = baseOffset + count * ENTITY_RECORD_SIZE;
            if (off + ENTITY_RECORD_SIZE > SNAPSHOT_LIMIT) {
                break;
            }

            stateBuffer.putInt(off, entity.getId());
            stateBuffer.putShort(off + 4, EntityKind.of(entity));
            stateBuffer.putShort(off + 6, buildEntityFlags(entity));
            stateBuffer.putDouble(off + 8, entity.getX());
            stateBuffer.putDouble(off + 16, entity.getY());
            stateBuffer.putDouble(off + 24, entity.getZ());

            Vec3 delta = entity.getDeltaMovement();
            stateBuffer.putFloat(off + 32, (float) delta.x);
            stateBuffer.putFloat(off + 36, (float) delta.y);
            stateBuffer.putFloat(off + 40, (float) delta.z);

            float health = entity instanceof LivingEntity living ? living.getHealth() : 0.0f;
            stateBuffer.putFloat(off + 44, health);
            count++;
        }

        return count;
    }

    private short buildEntityFlags(Entity entity) {
        short flags = 0;
        if (entity.isAlive()) {
            flags |= FLAG_ALIVE;
        }
        if (entity.onGround()) {
            flags |= FLAG_ON_GROUND;
        }
        if (entity.isInWater()) {
            flags |= FLAG_IN_WATER;
        }
        if (entity.isSprinting()) {
            flags |= FLAG_SPRINTING;
        }
        return flags;
    }

    private int writeChunkSections(ServerLevel level, int baseOffset, int maxSections) {
        int count = 0;
        var chunksToExport = new java.util.LinkedHashSet<Long>();

        for (Entity entity : level.getAllEntities()) {
            if (!(entity instanceof net.minecraft.world.entity.player.Player)) {
                continue;
            }

            int cx = entity.chunkPosition().x;
            int cz = entity.chunkPosition().z;

            for (int dx = -CHUNK_RADIUS; dx <= CHUNK_RADIUS; dx++) {
                for (int dz = -CHUNK_RADIUS; dz <= CHUNK_RADIUS; dz++) {
                    long key = ((long) (cx + dx) << 32) | ((cz + dz) & 0xFFFFFFFFL);
                    chunksToExport.add(key);
                }
            }
        }

        outer:
        for (long key : chunksToExport) {
            int cx = (int) (key >> 32);
            int cz = (int) key;

            LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
            if (chunk == null) {
                continue;
            }

            LevelChunkSection[] sections = chunk.getSections();
            int minSectionY = level.getMinSection();

            for (int index = 0; index < sections.length; index++) {
                if (count >= maxSections) {
                    break outer;
                }

                LevelChunkSection section = sections[index];
                if (section.hasOnlyAir()) {
                    continue;
                }

                int off = baseOffset + count * CHUNK_SECTION_RECORD_SIZE;
                int sectionY = minSectionY + index;
                stateBuffer.putInt(off, cx);
                stateBuffer.putInt(off + 4, sectionY);
                stateBuffer.putInt(off + 8, cz);
                stateBuffer.putInt(
                    off + 12,
                    section.maybeHas((BlockState state) -> !state.getFluidState().isEmpty()) ? 2 : 0
                );

                int blockDataOff = off + 16;
                for (int ly = 0; ly < 16; ly++) {
                    for (int lz = 0; lz < 16; lz++) {
                        for (int lx = 0; lx < 16; lx++) {
                            int idx = (ly << 8) | (lz << 4) | lx;
                            int rawId = Block.getId(section.getBlockState(lx, ly, lz));
                            stateBuffer.putInt(blockDataOff + idx * 4, rawId);
                        }
                    }
                }

                count++;
            }
        }

        return count;
    }
}
