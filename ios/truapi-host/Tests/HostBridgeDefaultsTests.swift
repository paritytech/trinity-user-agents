import Foundation
import Testing
import TrUAPIHost

struct HostBridgeDefaultsTests {
    /// An app that never implemented cards must say so, not pretend it moved or turned one.
    @Test
    func defaultExpandedCardIsUnsupported() async throws {
        let bridge: HostBridge = StubHostBridge()

        let face = try await bridge.setExpandedCardFaceShown(shown: false)
        let back = try await bridge.setExpandedCardBackShown(shown: true)

        #expect(face == .unsupported && back == .unsupported)
    }

    /// A host that resolves and picks contacts but never lists usernames must
    /// say so, not hand the game product an empty list.
    @Test
    func defaultContactUsernamesIsUnsupported() throws {
        let bridge: ContactsHostBridge = LookupAndPickContactsBridge()

        let listing = try bridge.contactUsernames(productId: ProductId("dim2.dot"))

        #expect(listing == .unsupported)
    }
}

private final class LookupAndPickContactsBridge: ContactsHostBridge, @unchecked Sendable {
    func contacts(lookup: HostContactLookup) throws -> HostContactMatches {
        HostContactMatches(accounts: lookup.handles.map { _ in nil })
    }

    func pickContact(productId _: ProductId) async throws -> HostContactPick {
        .dismissed
    }
}
