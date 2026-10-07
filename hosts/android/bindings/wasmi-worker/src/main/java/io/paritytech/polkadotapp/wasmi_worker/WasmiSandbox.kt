package io.paritytech.polkadotapp.wasmi_worker

/** One entry point the host asks the guest to run. */
sealed interface WasmiTurn {
    data object Start : WasmiTurn
    class Frame(val bytes: ByteArray) : WasmiTurn
    data object Suspend : WasmiTurn
    data object Resume : WasmiTurn
}

sealed class WasmiSandboxError(message: String) : Exception(message) {
    class ModuleRejected(cause: Throwable) : WasmiSandboxError("wasm module rejected: ${cause.message}")
    class Faulted(cause: Throwable) : WasmiSandboxError("wasm guest faulted: ${cause.message}")
    object Destroyed : WasmiSandboxError("wasm sandbox already destroyed")
}

/**
 * Kotlin boundary over the native sandbox. Owns one guest instance; every turn runs on the caller's
 * thread, one at a time. A faulted guest stays faulted until [destroy], which is what the native side
 * enforces, so a caller that keeps feeding frames after a failure gets failures back, not a revived
 * guest.
 */
class WasmiSandbox private constructor(private var handle: Long) {

    fun turn(turn: WasmiTurn): Result<List<ByteArray>> {
        val current = handle
        if (current == DESTROYED) return Result.failure(WasmiSandboxError.Destroyed)
        val (kind, frame) = when (turn) {
            WasmiTurn.Start -> KIND_START to null
            is WasmiTurn.Frame -> KIND_FRAME to turn.bytes
            WasmiTurn.Suspend -> KIND_SUSPEND to null
            WasmiTurn.Resume -> KIND_RESUME to null
        }
        return runCatching { WasmiWorkerNative.turn(current, kind, frame).toList() }
            .recoverCatching { throw WasmiSandboxError.Faulted(it) }
    }

    fun takeLogs(): List<String> {
        val current = handle
        if (current == DESTROYED) return emptyList()
        return WasmiWorkerNative.takeLogs(current).toList()
    }

    fun destroy() {
        val current = handle
        if (current == DESTROYED) return
        handle = DESTROYED
        WasmiWorkerNative.destroy(current)
    }

    companion object {
        private const val DESTROYED = 0L
        private const val KIND_START = 0
        private const val KIND_FRAME = 1
        private const val KIND_SUSPEND = 2
        private const val KIND_RESUME = 3

        fun create(module: ByteArray, fuelPerTurn: Long, memoryBytes: Long): Result<WasmiSandbox> =
            runCatching { WasmiSandbox(WasmiWorkerNative.create(module, fuelPerTurn, memoryBytes)) }
                .recoverCatching { throw WasmiSandboxError.ModuleRejected(it) }
    }
}
