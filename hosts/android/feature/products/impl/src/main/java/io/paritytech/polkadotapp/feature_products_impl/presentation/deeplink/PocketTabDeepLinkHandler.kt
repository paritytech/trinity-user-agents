package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject

// `polkadotapp://pocket` is for links from no product: `navigate_to` refuses the `polkadotapp:` scheme, so a product
// links to `/-/pocket` under its own name instead.
internal class PocketTabDeepLinkHandler @Inject constructor(
    private val accountRepository: AccountRepository,
    private val router: ProductsRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri) =
        data.scheme == DeepLinkHandler.APP_SCHEME && data.host == POCKET_HOST

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = runCancellableCatching {
        accountRepository.awaitAccountsInitialized()

        DeeplinkProcessingOutcome.Navigate { router.openPocket() }
    }

    private companion object {
        const val POCKET_HOST = "pocket"
    }
}
