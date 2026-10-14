import Foundation
import Operation_iOS
import Products
import SubstrateSdk
import TrUAPIHost

/// Resolves the contact handles a transaction names back to accounts, and draws
/// the picker. The list never leaves the host: the core asks about handles, and
/// a product gets 32 bytes for the one person the user picked.
///
/// Answers from the latest contact snapshot ``ContactsChangeNotifier`` hands
/// it, plus a handle index kept per handle key, so a lookup is a dictionary read
/// rather than a store fetch. Only before the first snapshot arrives does it
/// fetch, and the picker does that without blocking.
final class AppContactsHostBridge: ContactsHostBridge, @unchecked Sendable {
    private let repositoryFactory: ChatContactRepositoryMaking
    private let operationQueue: OperationQueue
    private let routerFacade: ProductRoutersFacadeProtocol

    private let lock = NSLock()
    /// Unblocked contacts' accounts, from the latest snapshot.
    private var accounts: [Data]?
    /// Handle to account under one handle key. Rebuilt when the key or the
    /// contacts change; the key changes with the session.
    private var index: (handleKey: Data, accountsByHandle: [Data: Data])?

    init(
        repositoryFactory: ChatContactRepositoryMaking,
        operationQueue: OperationQueue,
        routerFacade: ProductRoutersFacadeProtocol
    ) {
        self.repositoryFactory = repositoryFactory
        self.operationQueue = operationQueue
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
    /// picker is the chat contact search, which reads the same store, so who is
    /// offered is decided in one place.
    func pickContact(productId: String) async throws -> HostContactPick {
        if try await currentAccounts().isEmpty { return .noContacts }

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

        // Inline callback with no snapshot yet: the one case that fetches.
        let current = try snapshot ?? fetchUnblockedContactsBlocking().map(\.accountId)
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

    private func currentAccounts() async throws -> [Data] {
        let snapshot = lock.withLock { accounts }
        if let snapshot { return snapshot }
        return try await fetchUnblockedContacts().map(\.accountId)
    }

    private func fetchUnblockedContactsBlocking() throws -> [Chat.Contact] {
        let operation = makeFetchOperation()
        operationQueue.addOperations([operation], waitUntilFinished: true)
        return try Self.unblocked(operation.extractNoCancellableResultData())
    }

    private func fetchUnblockedContacts() async throws -> [Chat.Contact] {
        let operation = makeFetchOperation()
        return try await withCheckedThrowingContinuation { continuation in
            operation.completionBlock = {
                continuation.resume(with: Result {
                    try Self.unblocked(operation.extractNoCancellableResultData())
                })
            }
            operationQueue.addOperation(operation)
        }
    }

    private func makeFetchOperation() -> BaseOperation<[Chat.Contact]> {
        repositoryFactory
            .createRepository(forFilter: NSPredicate.isContact())
            .fetchAllOperation(with: RepositoryFetchOptions())
    }

    /// Blocked contacts are dropped here rather than in the predicate, so one
    /// rule decides it and a predicate change cannot quietly offer somebody the
    /// user refused.
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
final class ContactsChangeNotifier {
    private let queue = DispatchQueue(label: "io.parity.truapi.contacts-change")
    private let onSnapshot: ([Chat.Contact]) -> Void
    private let onRemoval: () -> Void
    private var knownAccounts: Set<AccountId>?
    private var subscription: AnyObject?

    init(
        dataProviderFactory: ChatContactDataProviderMaking,
        logger: LoggerProtocol,
        onSnapshot: @escaping ([Chat.Contact]) -> Void = { _ in },
        onRemoval: @escaping () -> Void
    ) {
        self.onSnapshot = onSnapshot
        self.onRemoval = onRemoval
        subscription = dataProviderFactory.subscribeContactsSnapshot(
            for: NSPredicate.isContact(),
            deliverOn: queue,
            update: { [weak self] contacts in
                self?.receive(contacts)
            },
            failure: { error in
                logger.error("contacts change subscription failed: \(error)")
            }
        )
    }

    private func receive(_ contacts: [Chat.Contact]) {
        onSnapshot(contacts)
        let current = Set(contacts.filter { !$0.isBlocked }.map(\.accountId))
        defer { knownAccounts = current }

        guard let knownAccounts, !knownAccounts.isSubset(of: current) else { return }
        onRemoval()
    }
}
