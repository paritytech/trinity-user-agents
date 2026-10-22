package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsBundledContent
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_api.model.WorkerKeepAlive
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.security.MessageDigest
import javax.inject.Inject
import javax.inject.Singleton

// The chat product ships in the APK (apps/chat-product in the verbosity repository). Its archive is
// installed as the dotNS content of worker.chat.<tld>, so the worker WebView loads it from the same
// origin a published release would use.
@Singleton
internal class BundledChatProduct @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val bundledContent: DotNsBundledContent,
    private val manifestParser: ManifestParser,
) {
    fun isChatProduct(productId: ProductId, tld: DotNsTld): Boolean = productId == ReservedProductIds.chat(tld)

    suspend fun install(tld: DotNsTld) {
        val files = withContext(Dispatchers.IO) {
            mapOf(ENTRYPOINT to asset(ENTRYPOINT), MANIFEST to asset(MANIFEST))
        }
        val digest = sha256(files.getValue(ENTRYPOINT))
        check(digest == BUNDLED_CHAT_WORKER_SHA256) {
            "bundled chat worker digest $digest does not match the pinned $BUNDLED_CHAT_WORKER_SHA256"
        }
        val workerHost = ProductManifest.hostOf(ReservedProductIds.chat(tld), ExecutableKind.WORKER)
        bundledContent.install(workerHost.value, digest, files)
    }

    /** The bundled worker, with the version its manifest declares. */
    suspend fun worker(tld: DotNsTld): ProductExecutable.Worker {
        val workerHost = ProductManifest.hostOf(ReservedProductIds.chat(tld), ExecutableKind.WORKER)
        val manifest = withContext(Dispatchers.IO) { asset(MANIFEST).decodeToString() }
        val parsed = manifestParser.parseExecutable(manifest, ExecutableKind.WORKER, workerHost).getOrNull()
        return parsed as? ProductExecutable.Worker ?: ProductExecutable.Worker(
            scriptUrl = "https://${workerHost.value}/$ENTRYPOINT",
            appVersion = SemVer.ZERO,
            includesChat = true,
            includesPocket = false,
            pocketCards = emptyList(),
            keepAlive = WorkerKeepAlive.FOREGROUND,
        )
    }

    fun resolved(tld: DotNsTld, worker: ProductExecutable.Worker): ResolvedProduct = ResolvedProduct(
        product = Product(id = ReservedProductIds.chat(tld), name = DISPLAY_NAME, icon = null),
        executables = Executables(app = null, widget = null, worker = worker),
    )

    private fun asset(name: String): ByteArray = context.assets.open("$ASSET_DIR/$name").use { it.readBytes() }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private companion object {
        const val ASSET_DIR = "person-chat"
        const val ENTRYPOINT = "index.js"
        const val MANIFEST = "manifest.json"
        const val DISPLAY_NAME = "Chats"
    }
}
