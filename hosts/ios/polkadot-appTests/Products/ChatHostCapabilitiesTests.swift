import Foundation
import Testing
@testable import polkadot_app

/// The capability list names only features that this build serves from end to end (07-LATER §4.2, S49).
struct ChatHostCapabilitiesTests {
    /// `CHAT_FEATURE_NAMES` of the core, in its order.
    private let rfcNames = [
        "product_source_rooms", "reaction_details", "hop_attachments", "call_buttons", "contact_rooms"
    ]

    @Test func everyNameIsAFeatureOfTheRfcOnceAndInItsOrder() {
        let served = ChatHostCapabilities.served

        #expect(Set(served).count == served.count)
        #expect(rfcNames.filter(served.contains) == served)
    }

    @Test func callButtonsAreListed() {
        #expect(ChatHostCapabilities.serves(ChatHostCapabilities.callButtons))
        #expect(ChatHostCapabilities.callButtons == "call_buttons")
    }

    /// A room of the chat product with a contact draws as the native person chat, with its photos and videos.
    @Test func thePersonChatFeaturesAreListed() {
        let drawn = ["contact_rooms", "hop_attachments"]

        #expect(drawn.allSatisfy(ChatHostCapabilities.served.contains))
    }
}
