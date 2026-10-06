package io.paritytech.polkadotapp.app.root.deeplink.videogame

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.PRODUCT_GAME_PATH
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.WAITING_ROOM_PATH
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.WEEKLY_GAME_HOST
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.isProductGameLink
import io.paritytech.polkadotapp.feature_videogame_impl.utils.VideoGameLaunchCoordinator
import kotlinx.coroutines.withContext
import javax.inject.Inject

class VideoGameDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val videoGameRouter: VideoGameRouter,
    private val videoGameLaunchCoordinator: VideoGameLaunchCoordinator,
    private val productGameReminder: ProductGameReminder,
    private val dotNsTldProvider: DotNsTldProvider,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        if (data.scheme != DeepLinkHandler.APP_SCHEME || data.host != WEEKLY_GAME_HOST) return false

        // The weekly game needs PERSONHOOD; the product game reminder is not tied to it.
        return FeatureOption.PERSONHOOD.isEnabled || data.isProductGameLink()
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.io) {
        runCancellableCatching {
            accountRepository.awaitAccountsInitialized()

            val path = data.pathSegments.firstOrNull()

            when (path) {
                PRODUCT_GAME_PATH -> data.pathSegments.getOrNull(1)
                    ?.let { openProductGame(ProductId.fromStoredValue(it)) }
                WAITING_ROOM_PATH -> withContext(coroutineDispatchers.main) {
                    videoGameRouter.openWeeklyGameBot()
                    videoGameLaunchCoordinator.launchGame()
                }
                else -> withContext(coroutineDispatchers.main) {
                    videoGameRouter.openWeeklyGameBot()
                }
            }

            DeeplinkProcessingOutcome.NoOp
        }
    }

    // Only the game product holds a reminder, so a link naming any other product opens nothing.
    private suspend fun openProductGame(productId: ProductId) {
        val tld = dotNsTldProvider.getTld().logFailure("game product deeplink tld").getOrNull() ?: return
        if (productId != ReservedProductIds.game(tld)) return
        videoGameRouter.openGameProduct(productId)
        productGameReminder.cancel(productId)
    }
}
