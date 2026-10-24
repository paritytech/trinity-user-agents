package io.paritytech.polkadotapp.app.root.deeplink.videogame

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderOpenSource
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.PRODUCT_GAME_STARTS_AT_PARAM
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.WEEKLY_GAME_HOST
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.isProductGameLink
import kotlinx.coroutines.withContext
import javax.inject.Inject

class VideoGameDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val videoGameRouter: VideoGameRouter,
    private val productGameReminder: ProductGameReminder,
    private val dotNsTldProvider: DotNsTldProvider,
    private val telemetry: GameReminderTelemetry,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean =
        data.scheme == DeepLinkHandler.APP_SCHEME && data.host == WEEKLY_GAME_HOST && data.isProductGameLink()

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.io) {
        runCancellableCatching {
            val productId = data.pathSegments.getOrNull(1)?.let(ProductId::fromStoredValue)
            val startsAtMillis = data.getQueryParameter(PRODUCT_GAME_STARTS_AT_PARAM)?.toLongOrNull()
            productId?.let { tapReminder(it, startsAtMillis) }

            accountRepository.awaitAccountsInitialized()

            productId?.let { openProductGame(it, startsAtMillis) }

            DeeplinkProcessingOutcome.NoOp
        }
    }

    // The reminder notification is this link's only producer. The reminder is dropped before the account wait,
    // so the auto-opener cannot open the tapped game again or report it missed.
    private suspend fun tapReminder(productId: ProductId, startsAtMillis: Long?) {
        telemetry.report(GameReminderEvent.Tapped(productId, startsAtMillis))
        productGameReminder.cancel(productId)
    }

    // Only the game product holds a reminder, so a link naming any other product opens nothing.
    private suspend fun openProductGame(productId: ProductId, startsAtMillis: Long?) {
        val tld = dotNsTldProvider.getTld().logFailure("game product deeplink tld").getOrNull() ?: return
        if (productId != ReservedProductIds.game(tld)) return
        videoGameRouter.openGameProduct(productId)
        telemetry.report(GameReminderEvent.ProductOpened(productId, startsAtMillis, GameReminderOpenSource.ALARM))
    }
}
