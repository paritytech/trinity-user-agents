import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

struct AppPersonChatHostBridgeTests {
    /// The product announces each paired device to contacts with the keys the native chat announces it with.
    @Test func pairedDevicesKeepTheirStatementAccountAndSessionKey() async throws {
        let device = HostPairedDevice(
            statementAccountId: Data(repeating: 1, count: 32),
            encryptionPublicKey: Data(repeating: 2, count: 32),
            name: "Desktop"
        )
        let bridge = AppPersonChatHostBridge(nodes: { ["wss://hop.example"] }, devices: { [device] })

        #expect(try await bridge.pairedDevices() == [device])
        #expect(try await bridge.hopNodes() == ["wss://hop.example"])
    }
}

struct ProductChatContactsTests {
    /// The rest of the app reads the chat product's contacts after a restart, and a product's new list leaves other
    /// products' contacts alone.
    @Test func contactsSurviveARestartAndReplaceOnlyTheirProduct() async throws {
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("contacts.json")
        let bob = HostChatContact(accountId: Data(repeating: 0xB0, count: 32), username: "bob.01", roomId: "dm-b0", blocked: false)
        let carol = HostChatContact(accountId: Data(repeating: 0xC0, count: 32), username: nil, roomId: "dm-c0", blocked: true)
        let bridge = AppPersonChatHostBridge(nodes: { [] }, devices: { [] }, contacts: ProductChatContacts(file: file))

        try await bridge.setContacts(productId: "chat.dot", contacts: [bob, carol])
        try await bridge.setContacts(productId: "other.dot", contacts: [bob])
        try await bridge.setContacts(productId: "other.dot", contacts: [])

        #expect(ProductChatContacts(file: file).contacts() == [
            .init(productId: "chat.dot", accountId: bob.accountId, username: "bob.01", roomId: "dm-b0", blocked: false),
            .init(productId: "chat.dot", accountId: carol.accountId, username: nil, roomId: "dm-c0", blocked: true),
        ])
    }
}
