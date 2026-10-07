package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.wasmi_worker.WasmiSandbox
import io.paritytech.polkadotapp.wasmi_worker.WasmiTurn
import javax.inject.Inject

/** Builds the guest a wasm worker runs as; the real one loads the native sandbox library. */
interface WorkerGuestFactory {
    fun create(module: ByteArray, fuelPerTurn: Long, memoryBytes: Long): Result<WorkerGuest>
}

class SandboxWorkerGuestFactory @Inject constructor() : WorkerGuestFactory {
    override fun create(module: ByteArray, fuelPerTurn: Long, memoryBytes: Long): Result<WorkerGuest> =
        WasmiSandbox.create(module, fuelPerTurn, memoryBytes).map(::SandboxWorkerGuest)
}

private class SandboxWorkerGuest(private val sandbox: WasmiSandbox) : WorkerGuest {
    override fun turn(turn: WasmiTurn): Result<List<ByteArray>> = sandbox.turn(turn)

    override fun takeLogs(): List<String> = sandbox.takeLogs()

    override fun destroy() = sandbox.destroy()
}
