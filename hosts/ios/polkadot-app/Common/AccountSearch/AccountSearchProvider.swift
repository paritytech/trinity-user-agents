import Foundation
import SubstrateSdk
import AsyncExtensions
import StructuredConcurrency
import os
import Operation_iOS
import SDKLogger

final class AccountSearchProvider<RecentPayload: Sendable>: AccountSearching {
    typealias MatchPayload = ContactSearchPayload

    private let recentRowsStream: @Sendable () -> AnyAsyncSequence<[SearchRow<RecentPayload>]>
    private let localContactSearch: LocalContactSearching
    private let remoteContactSearch: RemoteContactOperationMaking
    private let ownAccountId: AccountId
    private let logger: LoggerProtocol
    private let stateLock: OSAllocatedUnfairLock<State>
    private let sourcesChangedNotifier = SourcesChangedNotifier()

    init(
        recentRowsStream: @escaping @Sendable () -> AnyAsyncSequence<[SearchRow<RecentPayload>]>,
        localContactSearch: LocalContactSearching,
        remoteContactSearch: RemoteContactOperationMaking,
        ownAccountId: AccountId,
        logger: LoggerProtocol
    ) {
        self.recentRowsStream = recentRowsStream
        self.localContactSearch = localContactSearch
        self.remoteContactSearch = remoteContactSearch
        self.ownAccountId = ownAccountId
        self.logger = logger
        stateLock = OSAllocatedUnfairLock(initialState: State())
    }

    deinit {
        replaceRecentSubscriptionTask(with: nil)
        sourcesChangedNotifier.finish()
    }

    func setup() {
        subscribeToRecent()
    }

    func sourcesChanged() -> AnyAsyncSequence<Void> {
        sourcesChangedNotifier.sequence()
    }

    func searchPhases(
        query: String?
    ) -> AsyncThrowingStream<AccountSearchSections<RecentPayload, MatchPayload>, Error> {
        let (stream, continuation) = AsyncThrowingStream<Sections, Error>.makeStream()

        let task = Task { await emitPhases(query: query, continuation: continuation) }
        continuation.onTermination = { _ in task.cancel() }

        return stream
    }
}

private extension AccountSearchProvider {
    typealias Sections = AccountSearchSections<RecentPayload, MatchPayload>
    typealias PhaseContinuation = AsyncThrowingStream<Sections, Error>.Continuation

    struct State {
        var recentRows: [SearchRow<RecentPayload>] = []
        var recentSubscriptionTask: Task<Void, Never>?
        var cachedGlobal: CachedGlobal?
    }

    /// The last successful global rows and the normalized query they belong to, so a restart of
    /// the same or a related query keeps them on screen instead of blanking for one round trip.
    struct CachedGlobal {
        let query: String
        let rows: [SearchRow<MatchPayload>]
    }

    /// Everything a phase needs except the global rows, so successive phases of one search
    /// compose from the same local snapshot.
    struct SectionsContext {
        let query: String?
        let recent: [SearchRow<RecentPayload>]
        let contacts: [SearchRow<MatchPayload>]
        let excluded: Set<AccountId>

        func sections(global: AccountSearchGlobal<SearchRow<MatchPayload>> = .loaded([])) -> Sections {
            AccountSearchComposer.compose(
                query: query,
                recent: recent,
                contacts: contacts,
                global: global,
                excluding: excluded
            )
        }
    }

    func emitPhases(query: String?, continuation: PhaseContinuation) async {
        defer { continuation.finish() }

        do {
            if let query, !query.isEmpty {
                try await emitQueryPhases(query: query, continuation: continuation)
            } else {
                try await emitAllContactsPhase(continuation: continuation)
            }
        } catch {
            continuation.finish(throwing: error)
        }
    }

    /// An empty query lists every stored contact and skips the global lookup entirely.
    func emitAllContactsPhase(continuation: PhaseContinuation) async throws {
        async let blockedIds = fetchBlockedAccountIds()
        let contacts = try await fetchLocalContacts(matching: nil, accountId: nil)
        let excluded = try await blockedIds.union([ownAccountId])

        let context = SectionsContext(
            query: nil,
            recent: currentRecentRows(),
            contacts: contacts,
            excluded: excluded
        )

        continuation.yield(context.sections())
    }

    func emitQueryPhases(query: String, continuation: PhaseContinuation) async throws {
        let normalizedQuery = query.trimmingDot()
        let accountId = try? normalizedQuery.toAccountId()

        async let blockedIds = fetchBlockedAccountIds()
        let contacts = try await fetchLocalContacts(matching: normalizedQuery, accountId: accountId)
        let excluded = try await blockedIds.union([ownAccountId])

        let context = SectionsContext(
            query: query,
            recent: currentRecentRows(),
            contacts: contacts,
            excluded: excluded
        )

        // An account id lookup is an exact fetch rather than a prefix search, so cached prefix
        // rows say nothing about it.
        let preservedGlobal = accountId == nil ? cachedGlobalRows(for: normalizedQuery) : []

        continuation.yield(context.sections(global: .pending(preservedGlobal)))

        let global: [SearchRow<MatchPayload>]?
        do {
            global = try await fetchGlobalContacts(query: normalizedQuery, accountId: accountId)
        } catch {
            return
        }

        // asyncExecute() routes cancellation through the operation coordinator, which does not
        // guarantee a CancellationError, so a superseded search must not surface as a failure.
        guard !Task.isCancelled else {
            return
        }

        if let global {
            // An exact account id fetch is not a prefix search, so its rows cannot serve any later
            // prefix query: caching them under an address key is write-only state that can only mislead.
            if accountId == nil {
                storeCachedGlobal(rows: global, query: normalizedQuery)
            }

            continuation.yield(context.sections(global: .loaded(global)))
        } else {
            // A failed lookup must not leave stale rows standing as though they were fresh.
            clearCachedGlobal()
            continuation.yield(context.sections(global: .failed))
        }
    }

    func currentRecentRows() -> [SearchRow<RecentPayload>] {
        stateLock.withLock { $0.recentRows }
    }

    /// Global search matches by prefix, so results for one query are a subset of results for any
    /// shorter prefix of it: rows cached under a related query, refiltered, can only be incomplete,
    /// never wrong, which beats blanking the section on every keystroke.
    func cachedGlobalRows(for query: String) -> [SearchRow<MatchPayload>] {
        let lowercasedQuery = query.lowercased()

        return stateLock.withLock { state in
            guard let cached = state.cachedGlobal else { return [] }

            let cachedQuery = cached.query.lowercased()
            guard cachedQuery.hasPrefix(lowercasedQuery) || lowercasedQuery.hasPrefix(cachedQuery) else {
                return []
            }

            return cached.rows.filter { row in
                row.matchTerms.contains { term in
                    term.lowercased().hasPrefix(lowercasedQuery)
                }
            }
        }
    }

    func storeCachedGlobal(rows: [SearchRow<MatchPayload>], query: String) {
        stateLock.withLock { $0.cachedGlobal = CachedGlobal(query: query, rows: rows) }
    }

    func clearCachedGlobal() {
        stateLock.withLock { $0.cachedGlobal = nil }
    }

    func subscribeToRecent() {
        let task = Task { [weak self] in
            guard let self else { return }
            do {
                for try await rows in recentRowsStream() {
                    guard !Task.isCancelled else { return }
                    stateLock.withLock { $0.recentRows = rows }
                    sourcesChangedNotifier.notify()
                }
            } catch {
                logger.error("Recent subscription error: \(error)")
            }
        }
        replaceRecentSubscriptionTask(with: task)
    }

    /// Swaps the stored handle under the lock and cancels the displaced task outside it,
    /// so a second `setup()` cannot orphan a running subscription.
    func replaceRecentSubscriptionTask(with task: Task<Void, Never>?) {
        let previous = stateLock.withLock { state in
            let previous = state.recentSubscriptionTask
            state.recentSubscriptionTask = task
            return previous
        }

        previous?.cancel()
    }

    /// Blocked contacts are excluded from every section, including remote results
    /// that the local lookup never returns.
    func fetchBlockedAccountIds() async throws -> Set<AccountId> {
        let repository = localContactSearch.blockedContacts()

        let contacts = try await repository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        return Set(contacts.map(\.accountId))
    }

    /// An account id takes precedence over a username prefix, since an exact address match
    /// is never also a username. A nil prefix with no account id fetches every stored contact.
    func fetchLocalContacts(
        matching usernamePrefix: String?,
        accountId: AccountId?
    ) async throws -> [SearchRow<MatchPayload>] {
        let repository =
            if let accountId {
                localContactSearch.contact(accountId: accountId)
            } else if let usernamePrefix {
                localContactSearch.searchContacts(usernamePrefix: usernamePrefix)
            } else {
                localContactSearch.allContacts()
            }

        let contacts = try await repository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        return contacts
            .map { contact in
                SearchRow(
                    accountId: contact.accountId,
                    username: Username(value: contact.username),
                    matchTerms: [contact.username],
                    payload: .local(contact)
                )
            }
    }

    /// Returns nil when the lookup failed, so the caller can report the outcome to the UI.
    func fetchGlobalContacts(query: String, accountId: AccountId?) async throws -> [SearchRow<MatchPayload>]? {
        do {
            if let accountId {
                let account = try await remoteContactSearch.fetch(by: accountId)
                try Task.checkCancellation()

                return account.map { [makeRemoteRow(contact: $0)] } ?? []
            }

            let contacts = try await remoteContactSearch.search(by: query).asyncExecute()
            try Task.checkCancellation()

            return contacts.map { makeRemoteRow(contact: $0) }
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.warning("Global contact search failed, continuing without global results: \(error)")
            return nil
        }
    }

    func makeRemoteRow(contact: Chat.RemoteContact) -> SearchRow<MatchPayload> {
        SearchRow(
            accountId: contact.accountId,
            username: Username(value: contact.username),
            matchTerms: [contact.username],
            payload: .remote(contact)
        )
    }
}
