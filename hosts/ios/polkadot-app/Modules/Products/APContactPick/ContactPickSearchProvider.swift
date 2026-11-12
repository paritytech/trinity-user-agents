import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import AsyncExtensions

/// The contact search the product picker runs: the stored contacts, and a
/// query that filters them.
///
/// The chat search reaches the People chain for usernames it does not hold,
/// because there the point is to start a conversation with someone new. A
/// product is choosing among people the user already keeps, and a dotNS lookup
/// there would offer strangers as if the user knew them, so there is no global
/// section and no recents to rank.
final class ContactPickSearchProvider: AccountSearching {
    typealias RecentPayload = ContactSearchPayload
    typealias MatchPayload = ContactSearchPayload
    typealias Sections = AccountSearchSections<RecentPayload, MatchPayload>

    private let localContactSearch: LocalContactSearching
    private let ownAccountId: AccountId
    private let sourcesChangedNotifier = SourcesChangedNotifier()

    init(localContactSearch: LocalContactSearching, ownAccountId: AccountId) {
        self.localContactSearch = localContactSearch
        self.ownAccountId = ownAccountId
    }

    deinit {
        sourcesChangedNotifier.finish()
    }

    /// Nothing to subscribe to: the contact list is read per search, and the
    /// picker is open for one decision.
    func setup() {}

    func sourcesChanged() -> AnyAsyncSequence<Void> {
        sourcesChangedNotifier.sequence()
    }

    /// One phase: every section here is read locally, so there is no global
    /// result to stream in after the first.
    func searchPhases(
        query: String?
    ) -> AsyncThrowingStream<Sections, Error> {
        let (stream, continuation) = AsyncThrowingStream<Sections, Error>.makeStream()

        let task = Task {
            do {
                let sections = try await search(query: query)
                continuation.yield(sections)
                continuation.finish()
            } catch {
                continuation.finish(throwing: error)
            }
        }
        continuation.onTermination = { _ in task.cancel() }

        return stream
    }

    func search(query: String?) async throws -> AccountSearchSections<RecentPayload, MatchPayload> {
        // One read after the other: a picker opens once, so overlapping two
        // reads of the same store buys nothing and only widens what has to be
        // safe to touch from two tasks at once.
        let blockedIds = try await fetchBlockedAccountIds()
        let contacts = try await fetchContacts(matching: query)
        let excluded = blockedIds.union([ownAccountId])

        return AccountSearchComposer.compose(
            query: query,
            recent: [],
            contacts: contacts,
            global: .loaded([]),
            excluding: excluded
        )
    }
}

private extension ContactPickSearchProvider {
    /// Blocked contacts are excluded here as well as by the predicate, so one
    /// rule decides who is offerable and a predicate change cannot quietly
    /// surface somebody the user refused.
    func fetchBlockedAccountIds() async throws -> Set<AccountId> {
        // Held in a local: the fetch keeps only a weak reference to the
        // repository, so chaining off the temporary reads an empty list and
        // every blocked contact silently becomes offerable.
        let repository = localContactSearch.blockedContacts()
        let contacts = try await repository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        return Set(contacts.map(\.accountId))
    }

    /// An empty query lists every stored contact, which is what the picker
    /// opens on.
    func fetchContacts(matching query: String?) async throws -> [SearchRow<MatchPayload>] {
        let normalized = query?.trimmingDot()
        let repository =
            if let normalized, !normalized.isEmpty {
                localContactSearch.searchContacts(usernamePrefix: normalized)
            } else {
                localContactSearch.allContacts()
            }

        let contacts = try await repository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        return contacts.map { contact in
            SearchRow(
                accountId: contact.accountId,
                username: Username(value: contact.username),
                matchTerms: [contact.username],
                payload: .local(contact)
            )
        }
    }
}
