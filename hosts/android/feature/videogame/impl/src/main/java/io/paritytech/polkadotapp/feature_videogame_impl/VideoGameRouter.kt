package io.paritytech.polkadotapp.feature_videogame_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.model.GameIndex
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.gameResults.GameResultsPayload
import io.paritytech.polkadotapp.feature_videogame_impl.utils.VideoGameLaunchCoordinator

interface VideoGameRouter : ReturnableRouter {
    fun openChatWithPlayers(gameIndex: GameIndex)

    fun openVoting()

    /**
     * Should not be called directly from viewmodel, use [VideoGameLaunchCoordinator] instead,
     * since it handles permission requests
     */
    fun openGamePlay()

    fun openUpgradeUsername()

    fun openVideoGameNotifications()

    suspend fun openWeeklyGameBot()

    fun openChatFeed(payload: ChatFeedPayload)

    fun openGameResults(payload: GameResultsPayload)

    suspend fun openGameProduct(productId: ProductId)
}
