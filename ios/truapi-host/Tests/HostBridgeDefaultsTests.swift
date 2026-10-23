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

        let first = try await bridge.beginOperation(productId: "test.dot", label: "funding")
        let second = try await bridge.beginOperation(productId: "test.dot", label: "")

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

    /// An app without coinage must say so, so the chat product keeps the keys
    /// of a received payment instead of dropping them.
    @Test
    func defaultPaymentReceivedIsUnsupported() async throws {
        let bridge: PersonChatHostBridge = NoCoinagePersonChat()
        let payment = HostPersonChatPaymentReceivedRequest(
            roomId: "dm-ab",
            messageId: "payment",
            timestamp: 1_700_000_000_000,
            totalValue: "5000000",
            coinKeys: [Data(repeating: 0x11, count: 64)]
        )

        let receipt = try await bridge.paymentReceived(productId: "chat.dot", payment: payment)

        #expect(receipt == .unsupported)
    }
}

private final class NoCoinagePersonChat: PersonChatHostBridge, @unchecked Sendable {
    func hopNodes() async throws -> [String] { [] }

    func pairedDevices() async throws -> [HostPairedDevice] { [] }

    func setContacts(productId _: String, contacts _: [HostChatContact]) async throws {}

    func historyExport(productId _: String, offset _: UInt64, maxLen _: UInt32) async throws -> HostHistoryExportChunk {
        HostHistoryExportChunk(total: 0, data: Data())
    }

    func historyImported(productId _: String) async throws {}
}
