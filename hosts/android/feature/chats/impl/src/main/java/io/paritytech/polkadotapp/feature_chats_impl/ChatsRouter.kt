package io.paritytech.polkadotapp.feature_chats_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.common.presentation.navigation.TabRouter
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload

interface ChatsRouter : ReturnableRouter, TabRouter {
    fun openChatFeed(payload: ChatFeedPayload)
    fun openChatSearch()
    fun openAddContact()
    fun openEnterAmount(payload: SendEnterAmountPayload)
    fun openMessageRequests()
}
