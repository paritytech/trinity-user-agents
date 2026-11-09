package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import uniffi.truapi.BundleReader
import uniffi.truapi.ExecutableKind
import uniffi.truapi.FundingProviderEntry
import uniffi.truapi.FundingProviderOrigin
import uniffi.truapi.ProductId
import uniffi.truapi.bundledAssetPath
import uniffi.truapi.bundledProducts as coreBundledProducts
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

/** The products this build ships. Debug, nightly and safetynet builds only; a build made without the bundle ships none. */
@Singleton
class BundledProducts internal constructor(
    private val files: BundledProductFiles,
    private val manifestParser: ManifestParser,
    private val enabled: Boolean,
) {
    @Inject
    internal constructor(
        files: BundledProductFiles,
        manifestParser: ManifestParser,
    ) : this(files, manifestParser, BuildConfig.BUNDLED_PRODUCTS_ENABLED)

    companion object {
        const val ROOT = "bundled-products"
        private const val WORKER_MANIFEST = "worker-manifest.json"
    }

    private class Shipped(val products: Map<ProductId, Listed>)

    private class Listed(val product: BundledProduct, val hasWorker: Boolean, val hasApp: Boolean)

    // The core decides what the bundle holds and which file serves a request; the app only reads its assets.
    // Listed once, on first use, so serving a request looks a path up instead of opening the asset to see
    // whether it is there.
    private val reader: AssetReader by lazy { AssetReader(files, if (enabled) ROOT else null) }

    private val shipped: Shipped by lazy {
        if (!enabled) return@lazy Shipped(emptyMap())

        val products = coreBundledProducts(ROOT, reader).associate { shipped ->
            shipped.productId to Listed(BundledProduct(shipped.productId, shipped.path), shipped.hasWorker, shipped.hasApp)
        }
        Shipped(products)
    }

    fun product(id: ProductId): BundledProduct? = shipped.products[id]?.product

    /** The bundled product whose App [url] opens, when it ships one. */
    fun appFor(url: String): BundledProduct? {
        val host = runCatching { URI(url).host }.getOrNull()?.lowercase(Locale.ROOT) ?: return null
        return ProductId.parse(host).getOrNull()?.let(::product)?.takeIf(::hasApp)
    }

    fun workerManifest(product: BundledProduct): String? = reader.readText("${product.root}/$WORKER_MANIFEST")

    fun hasWorker(product: BundledProduct): Boolean = shipped.products[product.id]?.hasWorker == true

    fun hasApp(product: BundledProduct): Boolean = shipped.products[product.id]?.hasApp == true

    /** The bundled products that serve Funding, as the core takes them: the manifest shipped is the one it goes by. */
    fun fundingProviders(): List<FundingProviderEntry> = shipped.products.values.sortedBy { it.id.value }.mapNotNull { product ->
        val manifest = workerManifest(product)
            ?.takeIf { hasWorker(product) && worker(product, it)?.includesFunding == true }
            ?: return@mapNotNull null
        FundingProviderEntry(productId = product.id, workerManifest = manifest, origin = FundingProviderOrigin.BUNDLED)
    }

    /** Whether the worker [product] ships declares the Chat modality; a manifest the parser rejects declares none. */
    fun includesChat(product: BundledProduct): Boolean =
        workerManifest(product)?.let { worker(product, it) }?.includesChat == true

    /** The file under [root] that a request for [requestPath] is served, by the core's rules for a published archive. */
    fun assetPath(root: String, requestPath: String, isMainFrame: Boolean): String? =
        if (enabled) bundledAssetPath(root, requestPath, isMainFrame, reader) else null

    fun open(path: String): InputStream? = files.open(path)

    // Read by the same parser a published worker manifest goes through.
    private fun worker(product: BundledProduct, manifest: String): ProductExecutable.Worker? =
        manifestParser.parseExecutable(manifest, ExecutableKind.WORKER, ExecutableHost(product.workerHost))
            .getOrNull() as? ProductExecutable.Worker
}

/** The core's view of the app's assets: every file under [root] listed once, so [exists] never opens one. */
private class AssetReader(private val files: BundledProductFiles, root: String?) : BundleReader {
    private val shipped: Set<String> = root?.let { filesUnder(it).toSet() }.orEmpty()

    override fun list(dir: String): List<String> = files.list(dir)

    override fun exists(path: String): Boolean = path in shipped

    override fun readText(path: String): String? = files.open(path)?.use { it.readBytes().decodeToString() }

    // The asset manager lists a directory's children and nothing under a file.
    private fun filesUnder(path: String): List<String> {
        val children = files.list(path)
        return if (children.isEmpty()) listOf(path) else children.flatMap { filesUnder("$path/$it") }
    }
}
