package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import uniffi.truapi.ExecutableKind
import uniffi.truapi.parseProductLink
import uniffi.truapi.productIdOwner
import javax.inject.Inject

/** Resolves the published executable archive, including explicit base-host apps without a manifest. */
class ProductServingHostResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val dotNsTldProvider: DotNsTldProvider,
) : DotNsServingHostResolver {
    override suspend fun servingHostFor(requestHost: String): Result<String> = servingHostFor(requestHost, ExecutableKind.APP)

    /** A page under a Pocket card uses the product's [ExecutableKind.WIDGET]. */
    suspend fun servingHostFor(requestHost: String, executable: ExecutableKind): Result<String> = runCancellableCatching {
        val tld = dotNsTldProvider.getTld().getOrThrow()
        val identifier = parseProductLink(requestHost, tld.value)?.identifier ?: return@runCancellableCatching requestHost
        val productId = productIdOwner(identifier)
        if (identifier != productId) return@runCancellableCatching identifier.value

        val resolved = resolveProductUseCase.resolve(productId).getOrThrow()
        val served = when (executable) {
            ExecutableKind.APP -> resolved.executables.app?.host
            ExecutableKind.WIDGET -> resolved.executables.widget?.host
            ExecutableKind.WORKER -> null
        }
        checkNotNull(served) { "Product $productId has no $executable page" }.value
    }
}
