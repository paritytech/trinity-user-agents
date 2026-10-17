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
/// - `preimage_attachments` (P10.3): the stager stages an attachment above 1 MiB through the core, but no staged
///   attachment reached a product on the simulator. The identity of the E2E harness is a new wallet with no
///   personhood, so the core gets no Bulletin allowance for the product, and the live chain stores at most 2 MiB
///   in one transaction (O32). The harness drives the path with `TRUAPI_IOS_E2E_STAGING`.
/// - `wake` (P10.4): no push route wakes a product yet.
/// - `room_calls` (P10.7): the runtime has no Media service.
enum ChatHostCapabilities {
    /// A user attachment above the inline limit is staged as a preimage (P10.3).
    static let preimageAttachments = "preimage_attachments"

    /// A user attachment above the inline limit is sealed: the core encrypts it and stores the ciphertext in parts.
    static let sealedAttachments = "sealed_attachments"

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
        "room_links",
        "sealed_attachments",
        "search",
        "bot_personas",
        "mute_archive",
        "threads",
        "branches",
        "reasoning_rows",
        "read_aloud",
        "call_buttons",
        "room_link_payloads"
    ]

    /// The header buttons, the ringing banner, the call bar and the call events (P10.7). The room screen draws a
    /// call bar without media: `room_calls` waits for the Media service (O29).
    static let callButtons = "call_buttons"

    static func serves(_ feature: String) -> Bool {
        served.contains(feature)
    }
}
