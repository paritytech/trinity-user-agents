package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Request
import javax.inject.Inject

sealed class WorkerModuleError(message: String) : Exception(message) {
    class Unreachable(url: String, cause: Throwable) : WorkerModuleError("worker module $url: ${cause.message}")
    class Rejected(url: String, code: Int) : WorkerModuleError("worker module $url answered HTTP $code")
    class TooLarge(url: String, size: Long) : WorkerModuleError("worker module $url is $size bytes, over the cap")
    class NotWasm(url: String) : WorkerModuleError("worker module $url is not a wasm binary")
}

/**
 * Reads a worker's wasm module from the URL its executable record names. The debug loop serves it
 * from a developer's machine; nothing here touches the WebView archive path.
 */
class WorkerModuleFetcher @Inject constructor(
    private val calls: Call.Factory,
    private val dispatchers: CoroutineDispatchers,
) {
    suspend fun fetch(script: WorkerScript): Result<ByteArray> = withContext(dispatchers.io) {
        val url = script.baseUrl.trimEnd('/') + "/" + script.entrypoint.trimStart('/')
        val response = runCatching { calls.newCall(Request.Builder().url(url).build()).execute() }
            .getOrElse { return@withContext Result.failure(WorkerModuleError.Unreachable(url, it)) }
        response.use {
            if (!it.isSuccessful) return@withContext Result.failure(WorkerModuleError.Rejected(url, it.code))
            val body = it.body
            val declared = body.contentLength()
            if (declared > MAX_MODULE_BYTES) return@withContext Result.failure(WorkerModuleError.TooLarge(url, declared))
            val bytes = body.byteStream().readNBytes(MAX_MODULE_BYTES + 1)
            if (bytes.size > MAX_MODULE_BYTES) return@withContext Result.failure(WorkerModuleError.TooLarge(url, bytes.size.toLong()))
            if (!bytes.startsWithWasmMagic()) return@withContext Result.failure(WorkerModuleError.NotWasm(url))
            Result.success(bytes)
        }
    }

    private fun ByteArray.startsWithWasmMagic(): Boolean =
        size >= WASM_MAGIC.size && WASM_MAGIC.indices.all { this[it] == WASM_MAGIC[it] }

    private companion object {
        const val MAX_MODULE_BYTES = 8 * 1024 * 1024
        val WASM_MAGIC = byteArrayOf(0x00, 0x61, 0x73, 0x6D)
    }
}
