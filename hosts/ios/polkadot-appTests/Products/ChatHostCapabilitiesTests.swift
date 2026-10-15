import Foundation
import Testing
@testable import polkadot_app

/// The capability list names only features that this build serves from end to end (07-LATER §4.2, S49).
struct ChatHostCapabilitiesTests {
    /// `CHAT_FEATURE_NAMES` of the core, in its order.
    private let rfcNames = [
        "product_source_rooms", "reaction_details", "participants", "message_menus", "participant_menus",
        "toolbar", "suggestions", "commands", "activity", "custom_bodies", "custom_footer", "inline_attachments",
        "payment_button", "room_links", "preimage_attachments", "search", "bot_personas", "mute_archive", "threads",
        "branches", "reasoning_rows", "read_aloud", "wake", "call_buttons", "room_calls"
    ]

    @Test func everyNameIsAFeatureOfTheRfcOnceAndInItsOrder() {
        let served = ChatHostCapabilities.served

        #expect(Set(served).count == served.count)
        #expect(rfcNames.filter(served.contains) == served)
    }

    @Test func theFeaturesThatTheAppDoesNotDrawYetAreNotListed() {
        let notDrawn = [
            "payment_button", "preimage_attachments", "search", "bot_personas", "mute_archive", "threads",
            "branches", "reasoning_rows", "read_aloud", "wake", "room_calls"
        ]

        #expect(!ChatHostCapabilities.served.contains(where: notDrawn.contains))
    }

    /// The call slice ships before the Media service (D31): the app lists `call_buttons` and never `room_calls`.
    @Test func callButtonsAreListedWithoutRoomCalls() {
        #expect(ChatHostCapabilities.serves(ChatHostCapabilities.callButtons))
        #expect(ChatHostCapabilities.callButtons == "call_buttons")
        #expect(!ChatHostCapabilities.serves("room_calls"))
    }
}
