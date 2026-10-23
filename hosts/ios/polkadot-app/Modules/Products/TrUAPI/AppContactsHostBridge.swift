import Foundation
import Products
import SubstrateSdk
import TrUAPIHost

/// Resolves the contact handles a transaction names back to accounts, and draws
/// the picker. The list never leaves the host: the core asks about handles, and
/// a product gets 32 bytes for the one person the user picked.
///
/// The contacts are those the chat product syncs to the host (``ProductContactsSource``), the only chat the app
/// runs. Answers from the latest contact snapshot ``ContactsChangeNotifier`` hands it, plus a handle index kept per
/// handle key, so a lookup is a dictionary read. Before the first snapshot arrives it reads the source, which keeps
/// the contacts in memory.
final class AppContactsHostBridge: ContactsHostBridge, @unchecked Sendable {
    private let source: ProductContactsSource
    private let routerFacade: ProductRoutersFacadeProtocol

    private let lock = NSLock()
    /// Unblocked contacts' accounts, from the latest snapshot.
    private var accounts: [Data]?
    /// Handle to account under one handle key. Rebuilt when the key or the
    /// contacts change; the key changes with the session.
    private var index: (handleKey: Data, accountsByHandle: [Data: Data])?

    init(source: ProductContactsSource, routerFacade: ProductRoutersFacadeProtocol) {
        self.source = source
        self.routerFacade = routerFacade
    }

    /// Take the latest contacts, dropping the handle index built from the old
    /// ones.
    func update(contacts: [Chat.Contact]) {
        let unblocked = Self.unblocked(contacts).map(\.accountId)
        lock.lock()
        accounts = unblocked
        index = nil
        lock.unlock()
    }

    /// A contact's handle is BLAKE2b-256 keyed with the lookup's handle key
    /// over its account. The index for that key is built once and reused until
    /// the contacts or the key change. The core re-checks each account returned.
    func contacts(lookup: HostContactLookup) throws -> HostContactMatches {
        let accountsByHandle = try handleIndex(for: lookup.handleKey)
        return HostContactMatches(accounts: lookup.handles.map { accountsByHandle[$0] })
    }

    /// Draws the picker over whatever is on screen and waits for the user.
    ///
    /// The names are read by the picker and never leave the host: what goes
    /// back is the account of the one person chosen, which the core turns into
    /// a handle. A dismissal is its own answer, so a product can tell "not now"
    /// from "this host has no picker".
    ///
    /// The list is read here only to answer an empty one without a sheet. The
    /// picker is the chat contact search over the same source, so who is
    /// offered is decided in one place.
    func pickContact(productId: String) async throws -> HostContactPick {
        if currentAccounts().isEmpty { return .noContacts }

        let picked = await withCheckedContinuation { continuation in
            Task { @MainActor [routerFacade] in
                let context = ContactPickContext(productId: productId)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showContactPick(context: context)
            }
        }

        guard let picked else { return .dismissed }
        return .picked(account: picked)
    }

    private func handleIndex(for handleKey: Data) throws -> [Data: Data] {
        lock.lock()
        if let index, index.handleKey == handleKey {
            defer { lock.unlock() }
            return index.accountsByHandle
        }
        let snapshot = accounts
        lock.unlock()

        // No snapshot yet: the source answers from memory.
        let current = snapshot ?? Self.unblocked(source.currentContacts()).map(\.accountId)
        var accountsByHandle: [Data: Data] = [:]
        for account in current {
            try accountsByHandle[account.blake2b32WithKey(handleKey)] = account
        }

        lock.lock()
        // Kept only if no newer snapshot landed while hashing.
        if accounts == snapshot {
            index = (handleKey, accountsByHandle)
        }
        lock.unlock()
        return accountsByHandle
    }

    private func currentAccounts() -> [Data] {
        let snapshot = lock.withLock { accounts }
        if let snapshot { return snapshot }
        return Self.unblocked(source.currentContacts()).map(\.accountId)
    }

    /// Blocked contacts are dropped here, so one rule decides it and a source
    /// change cannot quietly offer somebody the user refused.
    private static func unblocked(_ contacts: [Chat.Contact]) -> [Chat.Contact] {
        contacts.filter { !$0.isBlocked }
    }
}

/// Tells the core when a contact is removed or blocked, so a contact handle it
/// cached stops resolving.
///
/// Every snapshot also goes to `onSnapshot`, which keeps the bridge's lookup
/// index current. Only a shrinking set is reported as a removal: a cached handle names someone who was a
/// contact when it was cached, so a contact added since cannot make it wrong.
final class ContactsChangeNotifier: @unchecked Sendable {
    private let lock = NSLock()
    private let onSnapshot: ([Chat.Contact]) -> Void
    private let onRemoval: () -> Void
    private var knownAccounts: Set<AccountId>?
    private var observation: AnyObject?

    init(
        source: ProductContactsSource,
        onSnapshot: @escaping ([Chat.Contact]) -> Void = { _ in },
        onRemoval: @escaping () -> Void
    ) {
        self.onSnapshot = onSnapshot
        self.onRemoval = onRemoval
        // The contacts now are the baseline a later removal is measured from.
        receive(source.currentContacts())
        observation = source.observe { [weak self] contacts in
            self?.receive(contacts)
        }
    }

    private func receive(_ contacts: [Chat.Contact]) {
        onSnapshot(contacts)
        let current = Set(contacts.filter { !$0.isBlocked }.map(\.accountId))
        let removed = lock.withLock {
            defer { knownAccounts = current }
            guard let knownAccounts else { return false }
            return !knownAccounts.isSubset(of: current)
        }

        if removed {
            onRemoval()
        }
    }
}
