package io.paritytech.polkadotapp.feature_chats_api.domain

/**
 * The Chat features that this build serves from end to end, by their names in the feature table of the Chat
 * Extension v2 additions RFC. A product reads them through `capabilities` before it declares a room.
 *
 * Add a name in the change that ships its feature. The core removes each name that it cannot back.
 */
object ChatHostCapabilities {
    /** Rooms whose product keeps the messages. */
    const val PRODUCT_SOURCE_ROOMS = "product_source_rooms"

    /**
     * A user photo, video or file of the chat product goes to a HOP node as the native chat sends one, up to 128 MiB.
     * The core backs it for the chat product only.
     */
    const val HOP_ATTACHMENTS = "hop_attachments"

    /** The call buttons of the header, a ringing call and the call events. */
    const val CALL_BUTTONS = "call_buttons"

    /** Rooms with a contact, which the host draws as its own person chats. The core backs it for the chat product only. */
    const val CONTACT_ROOMS = "contact_rooms"

    /** The names in the order of the feature table of the RFC. */
    val features: List<String> = listOf(
        PRODUCT_SOURCE_ROOMS,
        HOP_ATTACHMENTS,
        CALL_BUTTONS,
        CONTACT_ROOMS,
    )
}
