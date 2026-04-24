package dev.graphite.host.commands;

public final class CommandType {
    private CommandType() {
    }

    public static final byte SET_BLOCK = 0x01;
    public static final byte SEND_CHAT = 0x02;
    public static final byte SPAWN_PARTICLE = 0x03;
    public static final byte SET_VELOCITY = 0x04;
    public static final byte KILL_ENTITY = 0x05;

    public static final int CQ_HEAD_OFFSET = 0;
    public static final int CQ_TAIL_OFFSET = 4;
    public static final int CQ_CAPACITY_OFFSET = 8;
    public static final int CQ_DATA_OFFSET = 16;
}
