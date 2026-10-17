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

    /** Who reacted, through `reactions_query`: S32. */
    const val REACTION_DETAILS = "reaction_details"

    /** The participant count, the pages and the authors by id, through `participants_query`: S30. */
    const val PARTICIPANTS = "participants"

    /** The message menus of the product, through `message_actions_query`: S25, S26 and S28. */
    const val MESSAGE_MENUS = "message_menus"

    /** The participant sheet of the product, through `participant_actions_query`: S31. */
    const val PARTICIPANT_MENUS = "participant_menus"

    /** The toolbar that `set_room_toolbar` declares: S29. */
    const val TOOLBAR = "toolbar"

    /** The quick replies that `set_room_suggestions` declares: S38. */
    const val SUGGESTIONS = "suggestions"

    /** The `/` catalogue that `set_room_commands` declares: S39. */
    const val COMMANDS = "commands"

    /** The activity line that `set_room_activity` declares: S40. */
    const val ACTIVITY = "activity"

    /** `Custom` bodies that the product draws through its renderer: S7. */
    const val CUSTOM_BODIES = "custom_bodies"

    /** The `Custom` footer that the product draws through its renderer: S35. */
    const val CUSTOM_FOOTER = "custom_footer"

    /** Photos, videos and files of the user sent inline, up to 1 MiB: S36. */
    const val INLINE_ATTACHMENTS = "inline_attachments"

    /** The in-room search, and `search_query` for a room whose product keeps the messages: S61 to S63. */
    const val SEARCH = "search"

    /** The mute and the archive of the user, which the product reads through `UserStateChanged`: S54 to S56. */
    const val MUTE_ARCHIVE = "mute_archive"

    /** A user attachment above 1 MiB goes to the Bulletin chain as a preimage that the product pays for: S57 to S60. */
    const val PREIMAGE_ATTACHMENTS = "preimage_attachments"

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

    /** The call buttons of the header, a ringing call, the call bar and the call events: S72 to S74. */
    const val CALL_BUTTONS = "call_buttons"

    /** A user file above 1 MiB and up to 32 MiB goes encrypted, as Bulletin parts that the product pays for. */
    const val SEALED_ATTACHMENTS = "sealed_attachments"

    /** The names in the order of the feature table of the RFC. */
    val features: List<String> = listOf(
        PRODUCT_SOURCE_ROOMS,
        REACTION_DETAILS,
        PARTICIPANTS,
        MESSAGE_MENUS,
        PARTICIPANT_MENUS,
        TOOLBAR,
        SUGGESTIONS,
        COMMANDS,
        ACTIVITY,
        CUSTOM_BODIES,
        CUSTOM_FOOTER,
        INLINE_ATTACHMENTS,
        SEARCH,
        BOT_PERSONAS,
        MUTE_ARCHIVE,
        THREADS,
        BRANCHES,
        REASONING_ROWS,
        READ_ALOUD,
        CALL_BUTTONS,
    )
}
