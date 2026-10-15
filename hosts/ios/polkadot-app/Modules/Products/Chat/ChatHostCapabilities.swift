import Foundation

/// The Chat features that this build of the app serves from end to end, by their names in the feature table of the
/// Chat Extension v2 additions RFC (07-LATER §4.2). The core answers `capabilities` with this list, less the names
/// that it cannot back.
///
/// Add a name in the change that ships its feature, and only when the scenario rows of the feature pass on the
/// simulator (S49). The launcher maps each name to its rows (`IOS_CHAT_FEATURE_ROWS` in
/// `scripts/lib/ios-chat-v2-scenarios.mjs`), so add the name there too.
///
/// Not listed, and why:
/// - `payment_button`: the input draws no payment button (P10.2).
/// - `preimage_attachments` (P10.3), `search` (P10.5), `bot_personas` (P10.6), `call_buttons` (P10.7),
///   `threads`, `branches`, `reasoning_rows`, `read_aloud` (P10.9), `mute_archive` (P10.10): the host keeps what the
///   product declares for them, but it does not draw them yet.
/// - `wake` (P10.4): no push route wakes a product yet.
/// - `room_calls` (P10.7): the runtime has no Media service.
enum ChatHostCapabilities {
    static let served: [String] = [
        "product_source_rooms",
        "reaction_details",
        "participants",
        "message_menus",
        "participant_menus",
        "toolbar",
        "suggestions",
        "commands",
        "activity",
        "custom_bodies",
        "custom_footer",
        "inline_attachments",
        "room_links"
    ]
}
