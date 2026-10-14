package io.paritytech.polkadotapp.app.root.deeplink.product

import android.net.Uri
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.asWebUri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.isPocketTarget
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class ProductSpaDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val rootRouter: RootRouter,
    private val dotNsTldProvider: DotNsTldProvider,
    private val gate: ProductDeepLinkGate,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false
        if (!DotNsUtils.isDotDomain(data.asWebUri(), tld)) return false
        // Pocket is the host's own target and is answered elsewhere; every other route under the
        // reserved segment still opens as an App page, as it did before Pocket claimed one.
        if (data.isPocketTarget()) return false

        return gate.opens(data.productId(tld))
    }

    private fun Uri.productId(tld: DotNsTld): ProductId? = ProductId.fromUrl(asWebUri(), tld).getOrNull()

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> =
        withContext(coroutineDispatchers.io) {
            dotNsTldProvider.getTld().mapCatching { tld ->
                accountRepository.awaitAccountsInitialized()

                val normalized = DotNsUtils.normalize(data.asWebUri(), tld)
                    ?: error("Not a $tld domain: $data")

                DeeplinkProcessingOutcome.Navigate {
                    rootRouter.openSpaBrowser(SpaBrowserPayload.ByUrl(normalized.toString()))
                }
            }
        }
}
