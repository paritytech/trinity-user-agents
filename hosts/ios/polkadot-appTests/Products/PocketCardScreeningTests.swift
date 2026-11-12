import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The rules belong to the core and are tested there. What matters here is that
/// the host reaches for them, and for the right one per field: a host screening
/// by its own rules refuses cards and links the core accepted.
struct PocketCardScreeningTests {
    /// An id is addressed, so it may not carry characters that let two ids draw
    /// alike. A title is only drawn, and emoji need exactly those characters.
    @Test func aTitleAcceptsTheEmojiAnIdRefuses() throws {
        let coffee = "\u{2615}\u{fe0f} Coffee"

        #expect(try PocketCardScreening.core.title(coffee) == coffee)
        #expect(throws: (any Error).self) { try PocketCardScreening.core.id(coffee) }
    }

    /// Both sides NFC-normalize, so a card added under one spelling is found
    /// again under the other.
    @Test func screeningNormalizesAndTrims() throws {
        #expect(try PocketCardScreening.core.id("  cafe\u{301}  ").value == "caf\u{e9}")
        #expect(throws: (any Error).self) { try PocketCardScreening.core.id("   ") }
    }

    /// A title long enough to crowd out the product's name is the view's
    /// problem, but the core still bounds what it will carry.
    @Test func screeningBoundsATitleTheCoreWillNotCarry() throws {
        #expect(throws: (any Error).self) {
            try PocketCardScreening.core.title(String(repeating: "a", count: 257))
        }
    }
}
