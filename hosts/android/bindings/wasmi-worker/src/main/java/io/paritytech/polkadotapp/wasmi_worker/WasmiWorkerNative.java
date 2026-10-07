package io.paritytech.polkadotapp.wasmi_worker;

public class WasmiWorkerNative {

    static {
        System.loadLibrary("wasmi_worker_java");
    }

    // Instantiates the module under a per-turn fuel budget and a linear-memory cap. Returns a
    // handle for the other calls; throws IllegalStateException when the module does not fit the
    // worker ABI.
    public static native long create(byte[] module, long fuelPerTurn, long memoryBytes);

    // Runs one turn: kind is 0 start, 1 frame (with bytes), 2 suspend, 3 resume. Returns the
    // frames the guest emitted, in order. Throws IllegalStateException on a trap, fuel exhaustion,
    // memory-cap refusal, or when the sandbox is poisoned by an earlier fault.
    public static native byte[][] turn(long handle, int kind, byte[] frame);

    // Lines the guest logged since the last call.
    public static native String[] takeLogs(long handle);

    public static native void destroy(long handle);
}
