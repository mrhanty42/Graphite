package dev.graphite.host;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

public final class SharedMemory {
    public static final int OFFSET_TICK_COUNTER = 0x0000;
    public static final int OFFSET_SNAPSHOT_READY = 0x0008;
    public static final int OFFSET_COMMAND_COUNT = 0x000C;
    public static final int OFFSET_WORLD_SNAPSHOT = 0x0040;
    public static final int OFFSET_COMMAND_QUEUE = 0x30000;
    public static final int OFFSET_EVENT_RING = 0x38000;
    public static final int TOTAL_SIZE = 0x40000;

    // Command Queue structure offsets (relative to OFFSET_COMMAND_QUEUE)
    private static final int CQ_HEAD_OFFSET = 0;
    private static final int CQ_TAIL_OFFSET = 4;
    private static final int CQ_CAPACITY_OFFSET = 8;
    private static final int CQ_DATA_OFFSET = 16;
    private static final int CQ_CAPACITY = 0x7FF0 - CQ_DATA_OFFSET;  // 32736 bytes

    private static final VarHandle INT_HANDLE = MethodHandles.byteBufferViewVarHandle(
        int[].class,
        ByteOrder.LITTLE_ENDIAN
    );
    private static final VarHandle LONG_HANDLE = MethodHandles.byteBufferViewVarHandle(
        long[].class,
        ByteOrder.LITTLE_ENDIAN
    );

    private final ByteBuffer stateBuffer;
    private final ByteBuffer eventRingBuffer;
    private final long stateAddress;
    private final long eventRingAddress;

    public SharedMemory() {
        this.stateBuffer = ByteBuffer.allocateDirect(TOTAL_SIZE).order(ByteOrder.LITTLE_ENDIAN);
        this.eventRingBuffer = ByteBuffer.allocateDirect(0x4000).order(ByteOrder.LITTLE_ENDIAN);
        this.stateAddress = NativeBridge.graphiteGetDirectBufferAddress(stateBuffer);
        this.eventRingAddress = NativeBridge.graphiteGetDirectBufferAddress(eventRingBuffer);

        if (this.stateAddress == 0 || this.eventRingAddress == 0) {
            throw new IllegalStateException("Failed to acquire direct buffer address");
        }
        
        // Initialize CommandQueue: head=0, tail=0, capacity=CQ_CAPACITY
        initializeCommandQueue();
    }

    /**
     * Initializes the CommandQueue in shared memory by setting head, tail, and capacity fields.
     * Must be called exactly once during SharedMemory initialization.
     */
    private void initializeCommandQueue() {
        // Set head = 0 at OFFSET_COMMAND_QUEUE + CQ_HEAD_OFFSET
        setIntRelease(stateBuffer, OFFSET_COMMAND_QUEUE + CQ_HEAD_OFFSET, 0);
        // Set tail = 0 at OFFSET_COMMAND_QUEUE + CQ_TAIL_OFFSET
        setIntRelease(stateBuffer, OFFSET_COMMAND_QUEUE + CQ_TAIL_OFFSET, 0);
        // Set capacity = CQ_CAPACITY at OFFSET_COMMAND_QUEUE + CQ_CAPACITY_OFFSET
        setIntRelease(stateBuffer, OFFSET_COMMAND_QUEUE + CQ_CAPACITY_OFFSET, CQ_CAPACITY);
    }

    public ByteBuffer getStateBuffer() {
        return stateBuffer.duplicate().order(ByteOrder.LITTLE_ENDIAN);
    }

    public ByteBuffer getEventRingBuffer() {
        return eventRingBuffer.duplicate().order(ByteOrder.LITTLE_ENDIAN);
    }

    public static int getIntAcquire(ByteBuffer buffer, int offset) {
        return (int) INT_HANDLE.getAcquire(buffer, offset);
    }

    public static void setIntRelease(ByteBuffer buffer, int offset, int value) {
        INT_HANDLE.setRelease(buffer, offset, value);
    }

    public static boolean compareAndSetInt(ByteBuffer buffer, int offset, int expected, int newValue) {
        return INT_HANDLE.compareAndSet(buffer, offset, expected, newValue);
    }

    public static long getLongAcquire(ByteBuffer buffer, int offset) {
        return (long) LONG_HANDLE.getAcquire(buffer, offset);
    }

    public static void setLongRelease(ByteBuffer buffer, int offset, long value) {
        LONG_HANDLE.setRelease(buffer, offset, value);
    }

    public static int getIntVolatile(ByteBuffer buffer, int offset) {
        return (int) INT_HANDLE.getVolatile(buffer, offset);
    }

    public static void setIntVolatile(ByteBuffer buffer, int offset, int value) {
        INT_HANDLE.setVolatile(buffer, offset, value);
    }

    public static long getLongVolatile(ByteBuffer buffer, int offset) {
        return (long) LONG_HANDLE.getVolatile(buffer, offset);
    }

    public static void setLongVolatile(ByteBuffer buffer, int offset, long value) {
        LONG_HANDLE.setVolatile(buffer, offset, value);
    }

    public long getStateAddress() {
        return stateAddress;
    }

    public long getEventRingAddress() {
        return eventRingAddress;
    }

    public int getStateSize() {
        return TOTAL_SIZE;
    }

    public int getEventRingSize() {
        return eventRingBuffer.capacity();
    }
}
