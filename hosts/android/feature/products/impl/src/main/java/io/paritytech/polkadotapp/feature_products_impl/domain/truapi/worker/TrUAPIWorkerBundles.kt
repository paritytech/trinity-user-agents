package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_dotns_api.domain.resolveToLocalFile
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import uniffi.truapi.WorkerBundle
import java.io.File
import java.security.MessageDigest
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrUAPIWorkerBundles @Inject internal constructor(
    @ApplicationContext context: Context,
    private val manifestParser: ManifestParser,
    private val dotNsResolver: DotNsResolver,
    private val tldProvider: DotNsTldProvider,
) {
    private val directory = context.noBackupFilesDir.resolve("truapi/worker-bundles")
    private val writes = Mutex()

    @OptIn(ExperimentalStdlibApi::class)
    suspend fun fetch(productId: String, contentHash: ByteArray?, workerUrlOverride: String?): WorkerBundle = withContext(Dispatchers.IO) {
        writes.withLock {
            if (contentHash != null) {
                val cached = directory.resolve(contentHash.toHexString())
                check(cached.isDirectory) { "Worker bundle missing for retained content hash" }
                return@withLock WorkerBundle(contentHash, cached.resolve(METADATA).readBytes(), cached.absolutePath)
            }
            val product = ProductId.fromStoredValue(productId)
            val manifestHost = ProductManifest.hostOf(product, ExecutableKind.WORKER)
            val executable = dotNsResolver.getMetadataEntry(manifestHost.value, ProductManifest.EXECUTABLE_RECORD_KEY).getOrThrow()
            val workerUrl = executable?.let {
                (manifestParser.parseExecutable(it, ExecutableKind.WORKER, manifestHost).getOrThrow() as ProductExecutable.Worker).scriptUrl
            } ?: workerUrlOverride ?: error("Product has no worker executable")
            val normalized = DotNsUtils.normalize(android.net.Uri.parse(workerUrl), tldProvider.getTld().getOrThrow())
                ?: throw uniffi.truapi.HostRejection.Rejected("Arbitrary HTTP development worker bundles are unsupported in this experiment; use a published dotNS bundle")
            val script = WorkerScript.of(normalized.toString())
            val host = android.net.Uri.parse(script.baseUrl).host ?: error("Worker URL has no host")
            val archive = dotNsResolver.resolveToLocalFile(host).getOrThrow()
            val entry = archive.resolve(script.entrypoint).canonicalFile
            require(entry.path.startsWith(archive.canonicalPath + File.separator) && entry.isFile) { "Worker entry escapes its bundle" }
            val metadata = buildJsonObject {
                put("scriptUrl", normalized.toString())
                put("manifest", executable)
            }.toString().encodeToByteArray()
            val digest = MessageDigest.getInstance("SHA-256")
            digest.update(java.nio.ByteBuffer.allocate(Long.SIZE_BYTES).putLong(metadata.size.toLong()).array())
            digest.update(metadata)
            archive.walkTopDown().filter { it.isFile }.sortedBy { it.relativeTo(archive).path }.forEach { file ->
                val path = file.relativeTo(archive).path.encodeToByteArray()
                digest.update(java.nio.ByteBuffer.allocate(Long.SIZE_BYTES).putLong(path.size.toLong()).array())
                digest.update(path)
                digest.update(java.nio.ByteBuffer.allocate(Long.SIZE_BYTES).putLong(file.length()).array())
                file.inputStream().use { input ->
                    val buffer = ByteArray(8192)
                    var count = input.read(buffer)
                    while (count >= 0) {
                        digest.update(buffer, 0, count)
                        count = input.read(buffer)
                    }
                }
            }
            val hash = digest.digest()
            val target = directory.resolve(hash.toHexString())
            if (!target.exists()) {
                directory.mkdirs()
                val staging = File.createTempFile("worker-", ".staging", directory)
                check(staging.delete() && staging.mkdir())
                try {
                    archive.copyRecursively(staging, overwrite = false)
                    staging.resolve(METADATA).writeBytes(metadata)
                    check(staging.renameTo(target)) { "Worker bundle installation failed" }
                } finally {
                    staging.deleteRecursively()
                }
            }
            WorkerBundle(hash, metadata, target.absolutePath)
        }
    }

    private companion object {
        const val METADATA = ".truapi-worker.json"
    }
}
