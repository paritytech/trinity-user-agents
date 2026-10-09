package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation

fun ProductChatRoom.toCoreChatRoom(): ChatRoom = ChatRoom(
    roomId = roomId,
    participatingAs = when (participatingAs) {
        RoomParticipation.ROOM_HOST -> ChatRoomParticipation.ROOM_HOST
        RoomParticipation.BOT -> ChatRoomParticipation.BOT
    },
)
