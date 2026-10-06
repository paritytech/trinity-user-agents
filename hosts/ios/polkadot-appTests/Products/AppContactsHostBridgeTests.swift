import Testing
import Foundation
import Products
import SubstrateSdk
import TrUAPIHost
import UIKitExt
@testable import polkadot_app

/// The bridge answers lookups from the latest contact snapshot and a handle
/// index per key. These pin that the index follows the snapshot and the key,
/// so a lookup never answers from contacts or a session that are gone.
struct AppContactsHostBridgeTests {
    private let key = Data(repeating: 0x11, count: 32)
    private let otherKey = Data(repeating: 0x12, count: 32)

    @Test("A handle resolves to the contact it hashes from")
    func resolvesAKnownHandle() throws {
        let bridge = makeBridge()
        bridge.update(contacts: [makeContact(accountId: alice), makeContact(accountId: bob)])

        let matches = try bridge.contacts(
            lookup: HostContactLookup(handleKey: key, handles: [handle(bob, key), Data(repeating: 0, count: 32)])
        )

        #expect(matches.accounts == [bob, nil])
    }

    @Test("A blocked contact resolves to nobody")
    func blockedContactDoesNotResolve() throws {
        let bridge = makeBridge()
        bridge.update(contacts: [makeContact(accountId: alice, isBlocked: true)])

        let matches = try bridge.contacts(lookup: HostContactLookup(handleKey: key, handles: [handle(alice, key)]))

        #expect(matches.accounts == [nil])
    }

    @Test("A new snapshot drops a removed contact from the index")
    func snapshotRebuildsTheIndex() throws {
        let bridge = makeBridge()
        bridge.update(contacts: [makeContact(accountId: alice)])
        _ = try bridge.contacts(lookup: HostContactLookup(handleKey: key, handles: [handle(alice, key)]))

        bridge.update(contacts: [])
        let matches = try bridge.contacts(lookup: HostContactLookup(handleKey: key, handles: [handle(alice, key)]))

        #expect(matches.accounts == [nil])
    }

    @Test("A new handle key is hashed afresh")
    func keyChangeRebuildsTheIndex() throws {
        let bridge = makeBridge()
        bridge.update(contacts: [makeContact(accountId: alice)])
        _ = try bridge.contacts(lookup: HostContactLookup(handleKey: key, handles: [handle(alice, key)]))

        let matches = try bridge.contacts(
            lookup: HostContactLookup(handleKey: otherKey, handles: [handle(alice, key), handle(alice, otherKey)])
        )

        #expect(matches.accounts == [nil, alice])
    }

    private func makeBridge() -> AppContactsHostBridge {
        AppContactsHostBridge(
            repositoryFactory: MockChatContactRepositoryFactory(),
            operationQueue: OperationQueue(),
            routerFacade: UnusedRouters()
        )
    }

    private func handle(_ account: Data, _ key: Data) -> Data {
        // swiftlint:disable:next force_try
        try! account.blake2b32WithKey(key)
    }
}

private let alice = Data(repeating: 0xA1, count: 32)
private let bob = Data(repeating: 0xB0, count: 32)

/// The lookup never presents UI, so nothing here is reached.
private final class UnusedRouters: ProductRoutersFacadeProtocol {
    var productsRouter: ProductsRouter { fatalError("lookups present no UI") }
    var navigationRouter: ProductsNavigationRouting { fatalError("lookups present no UI") }

    @MainActor func setPresentationView(_: ControllerBackedProtocol) {}
}

private func makeContact(accountId: AccountId, isBlocked: Bool = false) -> Chat.Contact {
    Chat.Contact(
        accountId: accountId,
        username: "test_user",
        publicKey: Data(repeating: 0, count: 32),
        pin: nil,
        pushId: nil,
        pushToken: nil,
        voipPushToken: nil,
        peerPlatform: nil,
        lastOwnToken: nil,
        voipLastOwnToken: nil,
        chatRequest: nil,
        ownKeyId: Chat.Contact.Own(signKeyId: "", encryptionKeyId: ""),
        imageData: nil,
        isBlocked: isBlocked,
        devices: [],
        pendingDevicesFanOut: false,
        addedAt: nil,
        acceptedAt: nil
    )
}
