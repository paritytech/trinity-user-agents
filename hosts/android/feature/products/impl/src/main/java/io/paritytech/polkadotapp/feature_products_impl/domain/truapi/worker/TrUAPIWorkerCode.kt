package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.HandInstalledWorkers
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import timber.log.Timber
import java.io.File
import java.net.URI
import javax.inject.Inject

/** The code of one worker: a script that runs in a hidden WebView, or a wasm module that the core runs. */
sealed interface TrUAPIWorkerCode {
    /** The worker in the log. */
    val name: String

    /** An entry module at [scriptUrl] that runs in a hidden WebView. */
    data class Script(val scriptUrl: String) : TrUAPIWorkerCode {
        override val name: String get() = scriptUrl
    }

    /** A module compiled to wasm that the core runs, with no WebView. */
    class Wasm(val module: ByteArray, override val name: String) : TrUAPIWorkerCode

    companion object {
        /** The first bytes of every wasm module: `\0asm`. */
        private val WASM_MAGIC = byteArrayOf(0x00, 0x61, 0x73, 0x6D)

        /** True when [header] starts with the wasm magic bytes. */
        fun isWasm(header: ByteArray): Boolean =
            header.size >= WASM_MAGIC.size && WASM_MAGIC.indices.all { header[it] == WASM_MAGIC[it] }

        /** True when the file [entry] starts with the wasm magic bytes. */
        fun isWasm(entry: File): Boolean {
            val header = ByteArray(WASM_MAGIC.size)
            val read = entry.inputStream().use { it.read(header) }
            return read == header.size && isWasm(header)
        }
    }
}

/**
 * Finds the code of a product's worker. The entry file decides: a file that starts with the wasm magic bytes is a
 * wasm module, and every other entry is a script, as before. A published worker needs no manifest change for this.
 */
class TrUAPIWorkerCodeSource @Inject constructor(
    private val handInstalled: HandInstalledWorkers,
    private val archive: ProductWorkerArchive,
) {
    suspend fun load(productId: ProductId, worker: ProductExecutable.Worker): Result<TrUAPIWorkerCode> = runCatching {
        handInstalled.entryFile(worker.scriptUrl)?.let { entry -> return@runCatching handInstalledCode(productId, entry) }
        publishedEntry(productId, worker.scriptUrl)?.let { (path, entry) ->
            if (TrUAPIWorkerCode.isWasm(entry)) return@runCatching TrUAPIWorkerCode.Wasm(entry.readBytes(), "${productId.value}/$path")
        }
        TrUAPIWorkerCode.Script(worker.scriptUrl)
    }

    // The WebView cannot read a local file, so a worker installed by hand runs only as a wasm module.
    private fun handInstalledCode(productId: ProductId, entry: File): TrUAPIWorkerCode {
        require(TrUAPIWorkerCode.isWasm(entry)) { "the worker installed by hand for ${productId.value} is not a wasm module" }
        return TrUAPIWorkerCode.Wasm(entry.readBytes(), "${productId.value}/${entry.parentFile?.name}/${entry.name}")
    }

    /**
     * The entry file of a published worker in its local archive, with its path. A worker URL on another host is not
     * published, for example a worker that a developer serves on the loopback address. When the archive cannot be
     * read, the worker stays a script, and the WebView reports the failure as before.
     */
    private suspend fun publishedEntry(productId: ProductId, scriptUrl: String): Pair<String, File>? {
        val uri = runCatching { URI(scriptUrl) }.getOrNull() ?: return null
        if (uri.host != ProductManifest.hostOf(productId, ExecutableKind.WORKER).value) return null
        val path = uri.path?.removePrefix("/")?.takeIf { it.isNotEmpty() } ?: return null
        return archive.file(productId, path)
            .onFailure { Timber.w(it, "TrUAPI worker entry of %s is not readable; it loads as a script", productId.value) }
            .getOrNull()
            ?.let { path to it }
    }
}
