import Testing
import Foundation
import Operation_iOS
import SubstrateSdk
@testable import polkadot_app

/// The core caches the contact handles it resolves, so the host has to say when
/// a cached one may have stopped naming a contact. These pin that it says so
/// exactly when somebody leaves the non-blocked set.
struct ContactsChangeNotifierTests {
    @Test("The first snapshot is a baseline, not a change")
    func firstSnapshotDoesNotNotify() {
        let (factory, removals, notifier) = makeNotifier()

        factory.deliver([makeContact(accountId: alice)])

        #expect(removals.isEmpty)
        _ = notifier
    }

    @Test("A contact added does not clear the cache")
    func additionDoesNotNotify() {
        let (factory, removals, notifier) = makeNotifier()

        factory.deliver([makeContact(accountId: alice)])
        factory.deliver([makeContact(accountId: alice), makeContact(accountId: bob)])

        #expect(removals.isEmpty)
        _ = notifier
    }

    @Test("A contact removed clears the cache")
    func removalNotifies() {
        let (factory, removals, notifier) = makeNotifier()

        factory.deliver([makeContact(accountId: alice), makeContact(accountId: bob)])
        factory.deliver([makeContact(accountId: alice)])

        #expect(removals.count == 1)
        _ = notifier
    }

    @Test("A contact blocked clears the cache")
    func blockNotifies() {
        let (factory, removals, notifier) = makeNotifier()

        factory.deliver([makeContact(accountId: alice)])
        factory.deliver([makeContact(accountId: alice, isBlocked: true)])

        #expect(removals.count == 1)
        _ = notifier
    }
}

private let alice = Data(repeating: 0xA1, count: 32)
private let bob = Data(repeating: 0xB0, count: 32)

private final class RemovalCounter {
    var count = 0

    /// SwiftFormat's `isEmpty` rule rewrites `count == 0` on anything with a
    /// `count`, so the type answers the spelling it is left with.
    var isEmpty: Bool { count == 0 }
}

private func makeNotifier() -> (FakeContactDataProviderFactory, RemovalCounter, ContactsChangeNotifier) {
    let factory = FakeContactDataProviderFactory()
    let removals = RemovalCounter()
    let notifier = ContactsChangeNotifier(dataProviderFactory: factory, logger: Logger.shared) {
        removals.count += 1
    }
    return (factory, removals, notifier)
}

/// Hands each snapshot straight to the subscriber, so a test drives the store.
private final class FakeContactDataProviderFactory: ChatContactDataProviderMaking {
    private var update: (([Chat.Contact]) -> Void)?

    func deliver(_ contacts: [Chat.Contact]) {
        update?(contacts)
    }

    func createAllContactsProvider() -> StreamableProvider<Chat.Contact> {
        fatalError("not used by the notifier")
    }

    func subscribeContactsSnapshot(
        for _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update: @escaping ([Chat.Contact]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        self.update = update
        return NSObject()
    }

    func subscribeChatsSnapshot(
        for _: NSPredicate?,
        deliverOn _: DispatchQueue,
        update _: @escaping ([Chat.LocalModel]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        NSObject()
    }
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
        source: .chat,
        isBlocked: isBlocked,
        devices: [],
        pendingDevicesFanOut: false,
        addedAt: nil,
        acceptedAt: nil
    )
}
