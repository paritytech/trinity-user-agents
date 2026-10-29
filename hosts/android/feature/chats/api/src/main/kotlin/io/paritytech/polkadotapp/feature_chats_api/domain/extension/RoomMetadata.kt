package io.paritytech.polkadotapp.feature_chats_api.domain.extension

class RoomMetadata(
    val name: String?,
    val icon: String?,
)

/** A new icon for a room. A null [icon] removes the icon of the room. */
data class RoomIconChange(val icon: String?)

class DefaultRoomMetadata(
    val name: String,
    val icon: String?,
)
