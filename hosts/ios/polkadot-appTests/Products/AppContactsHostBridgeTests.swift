import Testing
import Foundation
import Operation_iOS
import os
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

    @Test("Before a snapshot, a lookup reads the contacts the chat product synced")
    func readsTheProductContactsBeforeASnapshot() throws {
        let contacts = ProductChatContacts(file: emptyFile())
        try contacts.replace(productId: "chat.dot", contacts: [
            HostChatContact(accountId: alice, username: "alice", roomId: "dm-a1", blocked: false),
            HostChatContact(accountId: bob, username: nil, roomId: "dm-b0", blocked: true)
        ])
        let bridge = AppContactsHostBridge(source: contacts, routerFacade: UnusedRouters())

        let matches = try bridge.contacts(
            lookup: HostContactLookup(handleKey: key, handles: [handle(alice, key), handle(bob, key)])
        )

        #expect(matches.accounts == [alice, nil])
    }

    @Test("Each sync of the chat product reaches the notifier, so a removal clears the core's cache")
    func aSyncThatDropsAContactIsARemoval() throws {
        let contacts = ProductChatContacts(file: emptyFile())
        try contacts.replace(productId: "chat.dot", contacts: [
            HostChatContact(accountId: alice, username: "alice", roomId: "dm-a1", blocked: false)
        ])
        let removals = OSAllocatedUnfairLock(initialState: 0)
        let snapshots = OSAllocatedUnfairLock<[[Data]]>(initialState: [])
        let notifier = ContactsChangeNotifier(
            source: contacts,
            onSnapshot: { contacts in snapshots.withLock { $0.append(contacts.map(\.accountId)) } },
            onRemoval: { removals.withLock { $0 += 1 } }
        )

        try contacts.replace(productId: "chat.dot", contacts: [
            HostChatContact(accountId: alice, username: "alice", roomId: "dm-a1", blocked: true)
        ])

        #expect(removals.withLock { $0 } == 1)
        #expect(snapshots.withLock { $0 } == [[alice], [alice]])
        _ = notifier
    }

    @Test("The picker offers the unblocked contacts the chat product synced")
    func thePickerOffersTheSyncedContacts() async throws {
        let contacts = ProductChatContacts(file: emptyFile())
        try contacts.replace(productId: "chat.dot", contacts: [
            HostChatContact(accountId: alice, username: "alice", roomId: "dm-a1", blocked: false),
            HostChatContact(accountId: bob, username: "bob", roomId: "dm-b0", blocked: true)
        ])
        let search = ProductSyncedContactSearch(source: contacts)

        // Held in locals, as the picker holds them while it reads.
        let allRepository = search.allContacts()
        let blockedRepository = search.blockedContacts()
        let matchingRepository = search.searchContacts(usernamePrefix: "AL")
        let all = try await allRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
        let blocked = try await blockedRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
        let matching = try await matchingRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()

        #expect(all.map(\.accountId) == [alice])
        #expect(blocked.map(\.accountId) == [bob])
        #expect(matching.map(\.username) == ["alice"])
    }

    private func makeBridge() -> AppContactsHostBridge {
        AppContactsHostBridge(source: ProductChatContacts(file: emptyFile()), routerFacade: UnusedRouters())
    }

    private func emptyFile() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
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
