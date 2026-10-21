package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository

/** The call part of a product room for a test of the room screen: no call, and events that go nowhere. */
internal fun callFactory() = ProductRoomCallController.Factory(
    RealChatRoomCallRepository(), mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true),
)
