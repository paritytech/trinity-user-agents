package io.paritytech.polkadotapp.app.root.navigation.videogame

import dagger.Lazy
import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotData
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.model.GameIndex
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.chatWithPlayers.ChatWithPlayersPayload
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.gameResults.GameResultsPayload
import jakarta.inject.Inject
import kotlinx.coroutines.withContext

class VideoGameNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
    // Lazy breaks the cycle through RealProductSessionController's DeepLinkHandler.
    private val productSessionController: Lazy<ProductSessionController>,
    private val dispatchers: CoroutineDispatchers,
) : BaseNavigator(navigationHolder), VideoGameRouter {
    override fun openChatWithPlayers(gameIndex: GameIndex) {
        val payload = ChatWithPlayersPayload(gameIndex.value)
        performNavigation(
            actionId = R.id.action_global_to_chatWithPlayersFragment,
            args = payload.toPayloadBundle()
        )
    }

    override fun openVoting() = performNavigation(R.id.action_videoGamePlayFragment_to_videoGameVotingFragment)

    override fun openGamePlay() = performNavigation(R.id.action_global_to_video_game_play_graph)

    override fun openVideoGameNotifications() = performNavigation(R.id.action_global_to_video_game_notifications)

    override fun openUpgradeUsername() = performNavigation(R.id.action_global_to_upgradeUsernameFragment)

    override suspend fun openWeeklyGameBot() {
        val chatBotData = ChatBotData.weeklyGame()

        val payload = ChatFeedPayload.botChat(chatBotData.id)

        performNavigation(
            actionId = R.id.action_global_to_chatFeedFragment,
            args = payload.toPayloadBundle()
        )
    }

    override fun openChatFeed(payload: ChatFeedPayload) {
        performNavigation(
            actionId = R.id.action_global_to_chatFeedFragment,
            args = payload.toPayloadBundle()
        )
    }

    override fun openGameResults(payload: GameResultsPayload) {
        performNavigation(
            actionId = R.id.action_videoGameVotingFragment_to_gameResultsFragment,
            args = payload.toPayloadBundle()
        )
    }

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
