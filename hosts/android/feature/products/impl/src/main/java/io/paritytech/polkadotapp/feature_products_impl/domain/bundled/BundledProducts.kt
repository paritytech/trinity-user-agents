package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import io.paritytech.polkadotapp.feature_dotns_api.domain.ArchiveFiles
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.RootManifest
import uniffi.truapi.BundleEntry
import uniffi.truapi.ExecutableKind
import uniffi.truapi.FundingProviderEntry
import uniffi.truapi.ProductId
import uniffi.truapi.bundledFundingProviders
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.bundledProducts as coreBundledProducts

/** The files the app ships, by their path under its assets. */
interface BundledProductFiles : ArchiveFiles {
    /** Whether this build ships the bundle at all. */
    val enabled: Boolean

    companion object {
        const val ROOT = "bundled-products"
    }
}

/**
 * The products the app ships, run from their files under their own ids with no dotNS or Bulletin lookup. Each is
 * one directory under [BundledProductFiles.ROOT], named after the product, laid out as the core reads it.
 */
@Singleton
class BundledProducts @Inject internal constructor(
    val files: BundledProductFiles,
    private val manifestParser: ManifestParser,
) {
    val entries: List<BundleEntry> by lazy {
        if (files.enabled) coreBundledProducts(BundledProductFiles.ROOT, files) else emptyList()
    }

    fun entry(id: ProductId): BundleEntry? = entries.firstOrNull { it.productId == id }

    fun workerManifest(entry: BundleEntry): String? = files.readText("${entry.path}/$WORKER_MANIFEST")

    /** The bundled products that serve Funding, as the core reads them: the manifest shipped is the one it goes by. */
    fun fundingProviders(): List<FundingProviderEntry> =
        if (files.enabled) bundledFundingProviders(BundledProductFiles.ROOT, files) else emptyList()

    /**
     * [entry] resolved from its files as a published product resolves from dotNS: the root manifest it ships names it,
     * else its id does, and a manifest the parser rejects fails the resolve.
     */
    fun resolved(entry: BundleEntry): Result<ResolvedProduct> = runCatching {
        val root = files.readText("${entry.path}/$ROOT_MANIFEST")?.let { manifestParser.parseRoot(it).getOrThrow() }
            ?: RootManifest(displayName = entry.productId.value, description = null, icon = null)
        ResolvedProduct(
            product = Product(id = entry.productId, name = root.displayName, icon = root.icon),
            description = root.description,
            executables = Executables(
                app = if (entry.hasApp) ProductExecutable.App(ExecutableHost(entry.appHost), SemVer.ZERO) else null,
                widget = null,
                worker = if (entry.hasWorker) worker(entry) else null,
            ),
        )
    }

    private fun worker(entry: BundleEntry): ProductExecutable.Worker {
        val manifest = requireNotNull(workerManifest(entry)) { "Bundled ${entry.productId.value} ships no Worker manifest" }
        return manifestParser.parseExecutable(manifest, ExecutableKind.WORKER, ExecutableHost(entry.workerHost)).getOrThrow()
            as ProductExecutable.Worker
    }

    private companion object {
        const val ROOT_MANIFEST = "manifest.json"
        const val WORKER_MANIFEST = "worker-manifest.json"
    }
}

val BundleEntry.workerRoot: String
    get() = "$path/worker"

val BundleEntry.appRoot: String
    get() = "$path/app"

/** The worker's own origin, as a published worker is served from its `worker.` subname. */
val BundleEntry.workerHost: String
    get() = "worker.${productId.value}"

/** The App is served under the product id itself, which is the origin its grants and storage key off. */
val BundleEntry.appHost: String
    get() = productId.value
