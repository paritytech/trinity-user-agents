package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import uniffi.truapi.ExecutableKind
import uniffi.truapi.parseProductLink
import javax.inject.Inject

/** Resolves the published executable archive, including explicit base-host apps without a manifest. */
class ProductServingHostResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val dotNsTldProvider: DotNsTldProvider,
) : DotNsServingHostResolver {
    override suspend fun servingHostFor(requestHost: String): Result<String> = servingHostFor(requestHost, ExecutableKind.APP)

    /** A page under a Pocket card uses the product's [ExecutableKind.WIDGET], or its app when it publishes no widget. */
    suspend fun servingHostFor(
        requestHost: String,
        executable: ExecutableKind,
        retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
    ): Result<String> = dotNsTldProvider.getTld().flatMap { tld ->
        val identifier = parseProductLink(requestHost, tld.value)?.identifier ?: return@flatMap Result.success(requestHost)

        identifier.owner().flatMap { productId ->
            if (identifier != productId) return@flatMap Result.success(identifier.value)

            resolveProductUseCase.resolve(productId, retryPolicy)
                .map { resolved ->
                    when (executable) {
                        ExecutableKind.APP -> resolved.executables.app?.host
                        ExecutableKind.WIDGET -> resolved.executables.widget?.host ?: resolved.executables.app?.host
                        ExecutableKind.WORKER -> null
                    }?.value
                }
                .requireNotNull { IllegalStateException("Product $productId has no $executable page") }
        }
    }
}
