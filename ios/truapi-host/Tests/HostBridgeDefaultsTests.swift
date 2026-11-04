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

    /// An app that never implemented cards must say so, not pretend it moved or turned one.
    @Test
    func defaultExpandedCardIsUnsupported() async throws {
        let bridge: HostBridge = StubHostBridge()

        let face = try await bridge.setExpandedCardFaceShown(shown: false)
        let back = try await bridge.setExpandedCardBackShown(shown: true)

        #expect(face == .unsupported && back == .unsupported)
    }
}
