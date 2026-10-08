package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.flatten
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.RootManifest
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.toDefinition
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealResolveProductUseCase @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val manifestParser: ManifestParser,
    private val productRepository: ProductRepository,
    private val debugPocketCards: DebugPocketCards,
) : ResolveProductUseCase {
    // Successful resolutions only, so a transient read failure stays retryable. Hit without the
    // lock, since the WebView's request-interception thread blocks on this for every request.
    private val cache = ConcurrentHashMap<String, ResolvedProduct>()
    private val resolveMutex = Mutex()

    override suspend fun resolve(productId: ProductId): Result<ResolvedProduct> {
        cache[productId.value]?.let { return Result.success(it) }

        return resolveMutex.withLock {
            // Another caller may have resolved the same product while this one waited.
            cache[productId.value]?.let { return@withLock Result.success(it) }

            // runCatching guards the storage and coroutine machinery; the read itself fails as a Result.
            runCatching { resolveFromChain(productId) }
                .flatten()
                // Payload-free variants cannot carry the cause, and this is the last place with it.
                .mapErrorNotInstance<ResolvedProduct, ProductResolutionError> {
                    Timber.e(it, "Product resolution failed")
                    ProductResolutionError.Unknown
                }
                .onSuccess { cache[productId.value] = it }
        }
    }

    override suspend fun invalidate(productId: ProductId) {
        cache.remove(productId.value)
    }

    private suspend fun resolveFromChain(productId: ProductId): Result<ResolvedProduct> {
        val baseName = productId.value

        // A read failure is distinct from an absent record, which is the legacy path below.
        val rootText = dotNsResolver.getMetadataEntry(baseName, ProductManifest.ROOT_RECORD_KEY)
            .logFailure("Failed to read product manifest record for $baseName")
            .getOrElse { return Result.failure(it) }

        // Legacy products have no record, so one is synthesized to keep a single code path.
        val root = if (rootText == null) {
            RootManifest(displayName = baseName, icon = null)
        } else {
            manifestParser.parseRoot(rootText)
                .logFailure("Rejected product manifest for $baseName")
                .getOrElse { return Result.failure(ProductResolutionError.MalformedManifest) }
        }

        val userWorkerUrl = productRepository.getUserWorkerUrl(productId)
        val executables = collectExecutables(productId, hasManifest = rootText != null, userWorkerUrl = userWorkerUrl)
            .getOrElse { return Result.failure(it) }

        val product = Product(
            id = productId,
            name = root.displayName,
            icon = root.icon,
        )

        // Identity only — executables are re-fetched on the next resolve.
        productRepository.upsertResolvedProduct(product)

        return Result.success(ResolvedProduct(product = product, executables = executables))
    }

    // A manifest worker wins over the debug-menu `userWorkerUrl`; legacy products carry only an
    // APP rooted at the base name.
    private suspend fun collectExecutables(
        productId: ProductId,
        hasManifest: Boolean,
        userWorkerUrl: String?,
    ): Result<Executables> {
        if (!hasManifest) {
            return Result.success(
                Executables(
                    app = ProductExecutable.App(host = ExecutableHost(productId.value), appVersion = SemVer.ZERO),
                    widget = null,
                    worker = userWorkerUrl?.let { userSuppliedWorker(productId, it) },
                )
            )
        }

        return coroutineScope {
            val app = async { resolveExecutable(productId, ExecutableKind.APP) }
            val widget = async { resolveExecutable(productId, ExecutableKind.WIDGET) }
            val worker = async { resolveExecutable(productId, ExecutableKind.WORKER) }

            combineResults(app.await(), widget.await(), worker.await()) { appExec, widgetExec, workerExec ->
                Executables(
                    app = appExec as? ProductExecutable.App,
                    widget = widgetExec as? ProductExecutable.Widget,
                    worker = workerExec as? ProductExecutable.Worker
                        ?: userWorkerUrl?.let { userSuppliedWorker(productId, it) },
                )
            }
        }
    }

    /**
     * A worker served from a developer's machine. It carries a Pocket card when the debug menu names
     * one, which is what lets a card be tried without publishing a manifest first.
     */
    private fun userSuppliedWorker(productId: ProductId, scriptUrl: String): ProductExecutable.Worker {
        val card = debugPocketCards.get(productId)

        return ProductExecutable.Worker(
            scriptUrl = scriptUrl,
            appVersion = SemVer.ZERO,
            includesChat = true,
            includesPocket = card != null,
            pocketCards = listOfNotNull(card?.toDefinition()),
        )
    }

    // A read failure fails the whole resolution, so a transient one is retried rather than cached
    // as "this product has no such executable". An absent or rejected record is that answer.
    private suspend fun resolveExecutable(
        productId: ProductId,
        kind: ExecutableKind,
    ): Result<ProductExecutable?> {
        val host = ProductManifest.hostOf(productId, kind)

        return dotNsResolver.getMetadataEntry(host.value, ProductManifest.EXECUTABLE_RECORD_KEY)
            .logFailure("Failed to read executable record for $host")
            .map { text ->
                // Only the product owner can create modality subnames, so ownership needs no re-check.
                text?.let {
                    manifestParser.parseExecutable(it, kind, host)
                        .logFailure("Rejected executable record for $host")
                        .getOrNull()
                }
            }
    }
}
