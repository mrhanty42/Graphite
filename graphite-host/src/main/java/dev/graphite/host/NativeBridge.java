package dev.graphite.host;

import java.nio.ByteBuffer;

public final class NativeBridge {
    private NativeBridge() {
    }

    public static native void graphiteInit(
        long statePtr,
        long stateSize,
        long eventRingPtr,
        long eventRingSize,
        String modsDir
    );

    public static native void graphiteTick(long tickId);

    public static native void graphiteShutdown();

    public static native boolean graphiteReloadMod(String libPath);

    public static native String graphiteDebugInfo();

    public static native long graphiteGetDirectBufferAddress(ByteBuffer buffer);
}
