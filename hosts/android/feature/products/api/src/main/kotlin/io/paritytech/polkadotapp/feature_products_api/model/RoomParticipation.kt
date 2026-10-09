package io.paritytech.polkadotapp.feature_products_api.model

import com.google.gson.annotations.SerializedName

enum class RoomParticipation {
    @SerializedName("RoomHost")
    ROOM_HOST,

    @SerializedName("Bot")
    BOT,
}
