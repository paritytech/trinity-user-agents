package io.paritytech.polkadotapp.app.root.navigation.videogame

import dagger.Lazy
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import jakarta.inject.Inject
import kotlinx.coroutines.withContext

class VideoGameNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
    // Lazy breaks the cycle through RealProductSessionController's DeepLinkHandler.
    private val productSessionController: Lazy<ProductSessionController>,
    private val dispatchers: CoroutineDispatchers,
) : BaseNavigator(navigationHolder), VideoGameRouter {
    override suspend fun openGameProduct(productId: ProductId) = withContext(dispatchers.main) {
        if (isCurrentDestination(R.id.spaBrowserFragment)) {
            productSessionController.get().openProduct(productId)
        } else {
            performNavigation(
                actionId = R.id.action_global_to_spaBrowserFragment,
                args = SpaBrowserPayload.ByProductId(productId.value).toPayloadBundle(SpaBrowserPayload::class.java.name),
            )
        }
    }
}
