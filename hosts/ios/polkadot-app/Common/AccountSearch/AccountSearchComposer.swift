import SubstrateSdk
import Foundation

/// The global section's state together with its rows: `.failed` has no associated rows, so a
/// failed lookup cannot be represented as still carrying results.
enum AccountSearchGlobal<Row> {
    /// Rows preserved from a related query while the current lookup is in flight.
    case pending([Row])
    case loaded([Row])
    case failed

    var rows: [Row] {
        switch self {
        case let .pending(rows),
             let .loaded(rows):
            rows
        case .failed:
            []
        }
    }

    var isPending: Bool {
        guard case .pending = self else { return false }
        return true
    }

    var hasFailed: Bool {
        guard case .failed = self else { return false }
        return true
    }

    var isLoaded: Bool {
        guard case .loaded = self else { return false }
        return true
    }

    func map<Other>(_ transform: ([Row]) -> [Other]) -> AccountSearchGlobal<Other> {
        switch self {
        case let .pending(rows):
            .pending(transform(rows))
        case let .loaded(rows):
            .loaded(transform(rows))
        case .failed:
            .failed
        }
    }
}

struct AccountSearchSections<RecentPayload, MatchPayload> {
    let recent: [SearchRow<RecentPayload>]
    let contacts: [SearchRow<MatchPayload>]
    let global: AccountSearchGlobal<SearchRow<MatchPayload>>

    init(
        recent: [SearchRow<RecentPayload>],
        contacts: [SearchRow<MatchPayload>],
        global: AccountSearchGlobal<SearchRow<MatchPayload>> = .loaded([])
    ) {
        self.recent = recent
        self.contacts = contacts
        self.global = global
    }

    var hasContent: Bool {
        !recent.isEmpty || !contacts.isEmpty || !global.rows.isEmpty
    }
}

enum AccountSearchComposer {
    static func compose<RecentPayload, MatchPayload>(
        query: String?,
        recent: [SearchRow<RecentPayload>],
        contacts: [SearchRow<MatchPayload>],
        global: AccountSearchGlobal<SearchRow<MatchPayload>> = .loaded([]),
        excluding: Set<AccountId>,
        maxRecent: Int = 5
    ) -> AccountSearchSections<RecentPayload, MatchPayload> {
        let filteredRecents = recent.filter { !excluding.contains($0.accountId) }

        let recentSection: [SearchRow<RecentPayload>]
        if let query, !query.isEmpty {
            let lowercasedQuery = query.lowercased()
            let matched = filteredRecents.filter { row in
                row.matchTerms.contains { term in
                    term.lowercased().hasPrefix(lowercasedQuery)
                }
            }
            recentSection = Array(matched.prefix(maxRecent))
        } else {
            recentSection = Array(filteredRecents.prefix(maxRecent))
        }

        let recentIds = Set(recentSection.map(\.accountId))

        let filteredContacts = contacts.filter { !excluding.contains($0.accountId) }
        let dedupedContacts = filteredContacts.filter { !recentIds.contains($0.accountId) }
        let contactsSection = dedupedContacts.sorted { lhs, rhs in
            sortByUsername(lhs.username, rhs.username)
        }

        let allRecentAndContactIds = recentIds.union(dedupedContacts.map(\.accountId))

        return AccountSearchSections(
            recent: recentSection,
            contacts: contactsSection,
            global: global.map { rows in
                globalSection(
                    from: rows,
                    query: query,
                    excluding: excluding,
                    deduping: allRecentAndContactIds
                )
            }
        )
    }

    private static func globalSection<MatchPayload>(
        from rows: [SearchRow<MatchPayload>],
        query: String?,
        excluding: Set<AccountId>,
        deduping shownIds: Set<AccountId>
    ) -> [SearchRow<MatchPayload>] {
        guard let query, !query.isEmpty else {
            return []
        }

        return rows
            .filter { !excluding.contains($0.accountId) }
            .filter { !shownIds.contains($0.accountId) }
            .sorted { lhs, rhs in
                sortByUsername(lhs.username, rhs.username)
            }
    }

    private static func sortByUsername(_ lhs: Username?, _ rhs: Username?) -> Bool {
        switch (lhs, rhs) {
        case (.none, .none):
            false
        case (.none, _):
            false
        case (_, .none):
            true
        case let (lhsUsername?, rhsUsername?):
            lhsUsername < rhsUsername
        }
    }
}
