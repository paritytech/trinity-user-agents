package io.paritytech.polkadotapp.app.root.deeplink.product

import android.net.Uri
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMarks
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
import kotlinx.coroutines.withContext
import uniffi.truapi.ProductId
import javax.inject.Inject

internal class ProductSpaDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val rootRouter: RootRouter,
    private val dotNsTldProvider: DotNsTldProvider,
    private val gate: ProductDeepLinkGate,
    private val pageOpens: HostOpenMarks,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return false
        if (!DotNsUtils.isDotDomain(data, tld)) return false

        return gate.opens(data.productId(tld))
    }

    private fun Uri.productId(tld: DotNsTld): ProductId? = productIdFromUrl(this, tld).getOrNull()

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> {
        // Read before any hop: the product page's load is timed from here.
        val link = pageOpens.linkHandled()
        return withContext(coroutineDispatchers.io) {
            dotNsTldProvider.getTld().mapCatching { tld ->
                accountRepository.awaitAccountsInitialized()

                val normalized = DotNsUtils.normalize(data, tld)
                    ?: error("Not a $tld domain: $data")
                val productId = normalized.productId(tld)

                DeeplinkProcessingOutcome.Navigate {
                    productId?.let { pageOpens.linked(it.value, link) }
                    rootRouter.openSpaBrowser(SpaBrowserPayload.ByUrl(normalized.toString()))
                }
            }
        }
    }
}
