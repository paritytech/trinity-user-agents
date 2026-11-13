import Foundation
import Testing
import TrUAPIHost

struct HostBridgeDefaultsTests {
    /// A host that runs no workers still hands each operation its own id: the
    /// core counts demand per id, and a host overriding only `endOperation`
    /// would end every operation at once if they all shared one.
    @Test
    func defaultBeginOperationNamesEachOperationSeparately() async throws {
        let bridge = StubHostBridge()

        let first = try await bridge.beginOperation(productId: ProductId("test.dot"), label: "funding")
        let second = try await bridge.beginOperation(productId: ProductId("test.dot"), label: "")

        #expect(first != second)
        #expect(first != 0)
        #expect(second != 0)
    }

    /// An app that never implemented cards must say so, not pretend it moved one.
    @Test
    func defaultExpandedCardFaceIsUnsupported() async throws {
        let bridge: HostBridge = StubHostBridge()

        let outcome = try await bridge.setExpandedCardFaceShown(shown: false)

        #expect(outcome == .unsupported)
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
