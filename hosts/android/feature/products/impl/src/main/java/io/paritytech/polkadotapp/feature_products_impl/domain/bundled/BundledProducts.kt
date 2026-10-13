package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import uniffi.truapi.FundingProviderEntry
import uniffi.truapi.FundingProviderOrigin
import java.io.InputStream
import java.net.URI
import java.util.Locale
import javax.inject.Inject
import javax.inject.Singleton

/** The files the app ships, by their path under its assets. */
interface BundledProductFiles {
    /** The names directly under [path]; empty when nothing is there. */
    fun list(path: String): List<String>

    fun open(path: String): InputStream?
}

class AssetBundledProductFiles @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : BundledProductFiles {
    override fun list(path: String): List<String> =
        runCatching { context.assets.list(path)?.toList() }.getOrNull().orEmpty()

    override fun open(path: String): InputStream? = runCatching { context.assets.open(path) }.getOrNull()
}

/**
 * A product the app ships inside itself, run from its files with no dotNS or Bulletin lookup. Its pages
 * and its worker keep the product's own id, so what they ask of TrUAPI is attributed to it as if it had
 * been published.
 *
 * On disk it is one directory under [BundledProducts.ROOT], named after the product:
 *
 *     <product id>/
 *       worker-manifest.json   the Worker manifest, as it would be published
 *       worker/index.js        the Worker's entry module
 *       app/index.html         the App's pages
 */
data class BundledProduct(
    val id: ProductId,
    val root: String,
) {
    val workerRoot: String
        get() = "$root/worker"

    val appRoot: String
        get() = "$root/app"

    /** The worker's own origin, as a published worker is served from its `worker.` subname. */
    val workerHost: String
        get() = "worker.${id.value}"

    val workerScriptUrl: String
        get() = "https://$workerHost/$WORKER_ENTRY"

    /** The App is served under the product id itself, which is the origin its grants and storage key off. */
    val appHost: String
        get() = id.value

    companion object {
        const val WORKER_ENTRY = "index.js"
        const val APP_INDEX = "index.html"
    }
}

/** The products this build ships. Testnet builds only; a build made without the bundle ships none. */
@Singleton
class BundledProducts internal constructor(
    private val files: BundledProductFiles,
    private val enabled: Boolean,
) {
    @Inject
    constructor(files: BundledProductFiles) : this(files, BuildConfig.BUNDLED_PRODUCTS_ENABLED)

    companion object {
        const val ROOT = "bundled-products"
        private const val WORKER_MANIFEST = "worker-manifest.json"
    }

    private val json = Json { ignoreUnknownKeys = true }

    private val products: Map<ProductId, BundledProduct> by lazy {
        if (!enabled) return@lazy emptyMap()

        files.list(ROOT)
            .filterNot { it.startsWith(".") }
            .filter { name -> files.list("$ROOT/$name").isNotEmpty() }
            .associate { name -> ProductId.fromStoredValue(name) to BundledProduct(ProductId.fromStoredValue(name), "$ROOT/$name") }
    }

    fun product(id: ProductId): BundledProduct? = products[id]

    fun product(id: String): BundledProduct? = products[ProductId.fromStoredValue(id)]

    /** Every product shipped, ordered by id. */
    val all: List<BundledProduct>
        get() = products.values.sortedBy { it.id.value }

    /** The bundled product whose App [url] opens, when it ships one. */
    fun appFor(url: String): BundledProduct? {
        val host = runCatching { URI(url).host }.getOrNull()?.lowercase(Locale.ROOT) ?: return null
        return product(host)?.takeIf(::hasApp)
    }

    fun workerManifest(product: BundledProduct): String? =
        files.open("${product.root}/$WORKER_MANIFEST")?.use { it.readBytes().decodeToString() }

    fun hasWorker(product: BundledProduct): Boolean = exists("${product.workerRoot}/${BundledProduct.WORKER_ENTRY}")

    fun hasApp(product: BundledProduct): Boolean = exists("${product.appRoot}/${BundledProduct.APP_INDEX}")

    /** The bundled products that serve Funding, as the core takes them: the manifest shipped is the one it goes by. */
    fun fundingProviders(): List<FundingProviderEntry> = all.mapNotNull { product ->
        val manifest = workerManifest(product)?.takeIf { hasWorker(product) && includes(it, "funding") } ?: return@mapNotNull null
        FundingProviderEntry(productId = product.id.value, workerManifest = manifest, origin = FundingProviderOrigin.BUNDLED)
    }

    /** Whether the worker [product] ships declares the Chat modality. */
    fun includesChat(product: BundledProduct): Boolean = workerManifest(product)?.let { includes(it, "chat") } ?: false

    /**
     * The file under [root] that a request for [requestPath] is served, the way a published archive is:
     * the exact file, a directory's `index.html`, and for a page navigation the root `index.html`, which
     * lets an App route itself.
     */
    fun assetPath(root: String, requestPath: String, isMainFrame: Boolean): String? {
        val path = requestPath.trim('/')
        val index = "$root/${BundledProduct.APP_INDEX}"
        if (path.isEmpty()) return index.takeIf(::exists)
        // An escaping path is never served, whatever the asset manager would make of it.
        if (path.split('/').any { it == ".." }) return null

        val exact = "$root/$path"
        if (exists(exact)) return exact

        if ('.' !in path.substringAfterLast('/')) {
            val directoryIndex = "$exact/${BundledProduct.APP_INDEX}"
            if (exists(directoryIndex)) return directoryIndex
        }

        return index.takeIf { isMainFrame && exists(it) }
    }

    fun open(path: String): InputStream? = files.open(path)

    private fun exists(path: String): Boolean = files.open(path)?.use { true } ?: false

    private fun includes(manifest: String, modality: String): Boolean = runCatching {
        val includes = json.parseToJsonElement(manifest).jsonObject["includes"] as? JsonObject
        includes?.get(modality) != null
    }.getOrDefault(false)
}
