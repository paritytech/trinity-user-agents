package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation

class ProductChatRoom(
    val roomId: String,
    val participatingAs: RoomParticipation,
)
