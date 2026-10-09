package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter

class CreateProductRoomRequest(
    val chatIdParameter: ProductChatIdParameter,
    val name: String?,
    val icon: String?,
)

class CreateProductRoomResult(
    val status: CreateRoomStatus
)
