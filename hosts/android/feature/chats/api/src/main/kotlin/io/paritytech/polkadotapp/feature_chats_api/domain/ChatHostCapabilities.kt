package io.paritytech.polkadotapp.feature_chats_api.domain

/**
 * The Chat features that this build serves from end to end, by their names in the feature table of the Chat
 * Extension v2 additions RFC. A product reads them through `capabilities` before it declares a room.
 *
 * Add a name in the change that ships its feature, and only when the scenarios of the feature pass on the
 * emulator (04-TESTING S49). The core removes each name that it cannot back.
 */
object ChatHostCapabilities {
    /** Rooms whose product keeps the messages: S15 to S24 and S46. */
    const val PRODUCT_SOURCE_ROOMS = "product_source_rooms"

    /** `Custom` bodies that the product draws through its renderer: S7. */
    const val CUSTOM_BODIES = "custom_bodies"

    /** The in-room search, and `search_query` for a room whose product keeps the messages: S61 to S63. */
    const val SEARCH = "search"

    /** The mute and the archive of the user, which the product reads through `UserStateChanged`: S54 to S56. */
    const val MUTE_ARCHIVE = "mute_archive"

    val features: List<String> = listOf(PRODUCT_SOURCE_ROOMS, CUSTOM_BODIES)
}
