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
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.WEEKLY_GAME_HOST
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.isProductGameLink
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderTelemetry
import kotlinx.coroutines.withContext
import uniffi.truapi.ProductId
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
            val productId = data.pathSegments.getOrNull(1)?.let(ProductId::parse)
                ?: return@runCancellableCatching DeeplinkProcessingOutcome.NoOp
            val tld = dotNsTldProvider.getTld().logFailure("game product deeplink tld").getOrNull()
                ?: return@runCancellableCatching DeeplinkProcessingOutcome.NoOp
            if (productId != ReservedProductIds.game(tld)) return@runCancellableCatching DeeplinkProcessingOutcome.NoOp
            val startsAtMillis = productGameReminder.scheduledStartFor(productId)

            telemetry.report(GameReminderEvent.Tapped(productId, startsAtMillis))
            accountRepository.awaitAccountsInitialized()

            videoGameRouter.openGameProduct(productId)
            productGameReminder.cancel(productId)
            telemetry.report(GameReminderEvent.ProductOpened(productId, startsAtMillis))

            DeeplinkProcessingOutcome.NoOp
        }
    }
}
