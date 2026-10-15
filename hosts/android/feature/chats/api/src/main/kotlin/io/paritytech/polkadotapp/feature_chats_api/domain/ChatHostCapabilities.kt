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

    /** `register_bot` keeps a persona, and the host draws `bot:<id>` with its name and icon: S53. */
    const val BOT_PERSONAS = "bot_personas"

    /** Thread rooms, the chip under the root and the pinned root in the thread: S64. */
    const val THREADS = "threads"

    /** The branch control under a message and `BranchSelected`: S65. */
    const val BRANCHES = "branches"

    /** Reasoning rows in place of bubbles: S66 and S68. */
    const val REASONING_ROWS = "reasoning_rows"

    /** "Read aloud" in the message menu of a room that declares it: S67. */
    const val READ_ALOUD = "read_aloud"

    val features: List<String> = listOf(PRODUCT_SOURCE_ROOMS, CUSTOM_BODIES, BOT_PERSONAS, THREADS, BRANCHES, REASONING_ROWS, READ_ALOUD)
}
