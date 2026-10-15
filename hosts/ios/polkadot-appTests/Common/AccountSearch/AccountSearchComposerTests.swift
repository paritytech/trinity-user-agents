import Testing
import SubstrateSdk
import Foundation

@testable import polkadot_app

struct AccountSearchComposerTests {
    // MARK: - Empty Query Tests

    @Test("Empty query returns recents with maxRecent limit")
    func emptyQueryLimitsRecents() {
        let recent = createSearchRows(count: 10, hasUsername: true)
        let contacts: [SearchRow<Int>] = []
        let global: [SearchRow<Int>] = []

        let result = AccountSearchComposer.compose(
            query: nil,
            recent: recent,
            contacts: contacts,
            global: .loaded(global),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.recent.count == 5)
        #expect(result.contacts.isEmpty)
        #expect(result.global.rows.isEmpty)
    }

    @Test("Nil query returns recents with default maxRecent of 5")
    func nilQueryUsesDefaultMaxRecent() {
        let recent = createSearchRows(count: 8, hasUsername: true)
        let contacts: [SearchRow<Int>] = []
        let global: [SearchRow<Int>] = []

        let result = AccountSearchComposer.compose(
            query: nil,
            recent: recent,
            contacts: contacts,
            global: .loaded(global),
            excluding: []
        )

        #expect(result.recent.count == 5)
        #expect(result.contacts.isEmpty)
        #expect(result.global.rows.isEmpty)
    }

    @Test("Empty string query returns recents with maxRecent limit")
    func emptyStringQueryLimitsRecents() {
        let recent = createSearchRows(count: 7, hasUsername: true)
        let contacts: [SearchRow<Int>] = []
        let global: [SearchRow<Int>] = []

        let result = AccountSearchComposer.compose(
            query: "",
            recent: recent,
            contacts: contacts,
            global: .loaded(global),
            excluding: [],
            maxRecent: 3
        )

        #expect(result.recent.count == 3)
        #expect(result.contacts.isEmpty)
        #expect(result.global.rows.isEmpty)
    }

    // MARK: - Query Matching Tests

    @Test("Non-empty query filters recents by matchTerms prefix")
    func queryFiltersRecentsByMatchTerms() {
        let alice = createSearchRow(username: Username(value: "alice"), matchTerms: ["alice", "alice.01"])
        let bob = createSearchRow(username: Username(value: "bob"), matchTerms: ["bob", "bob.02"])
        let charlie = createSearchRow(
            username: Username(value: "charlie"),
            matchTerms: ["charlie", "charlie.03"]
        )

        let result = AccountSearchComposer.compose(
            query: "ali",
            recent: [alice, bob, charlie],
            contacts: [SearchRow<Int>](),
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.recent.count == 1)
        #expect(result.recent[0].username == Username(value: "alice"))
    }

    @Test("Query matching is case-insensitive")
    func queryMatchingIsCaseInsensitive() {
        let alice = createSearchRow(username: Username(value: "Alice"), matchTerms: ["alice"])
        let bob = createSearchRow(username: Username(value: "Bob"), matchTerms: ["bob"])

        let result = AccountSearchComposer.compose(
            query: "ALI",
            recent: [alice, bob],
            contacts: [SearchRow<Int>](),
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.recent.count == 1)
        #expect(result.recent[0].username == Username(value: "Alice"))
    }

    @Test("Query requires prefix match on matchTerms")
    func queryRequiresPrefixMatch() {
        let alice = createSearchRow(username: Username(value: "alice"), matchTerms: ["alice"])

        let result = AccountSearchComposer.compose(
            query: "lice",
            recent: [alice],
            contacts: [SearchRow<Int>](),
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.recent.isEmpty)
    }

    // MARK: - Deduplication Tests

    @Test("Contacts dedupe against recent accountIds")
    func contactsDedupeAgainstRecents() throws {
        let recentId = try Data.randomOrError(of: 32)
        let recentRow = createSearchRow(
            accountId: recentId,
            username: Username(value: "alice"),
            matchTerms: ["alice"]
        )

        let contactId = recentId // Same ID
        let contactRow = createSearchRow(
            accountId: contactId,
            username: Username(value: "alice.01")
        )

        let result = AccountSearchComposer.compose(
            query: "ali",
            recent: [recentRow],
            contacts: [contactRow],
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.contacts.isEmpty)
    }

    @Test("Global dedupe against recent and contact accountIds")
    func globalDedupeAgainstRecentAndContacts() throws {
        let recentId = try Data.randomOrError(of: 32)
        let recentRow = createSearchRow(
            accountId: recentId,
            username: Username(value: "alice"),
            matchTerms: ["search"]
        )

        let contactId = try Data.randomOrError(of: 32)
        let contactRow = createSearchRow(
            accountId: contactId,
            username: Username(value: "bob")
        )

        let globalId1 = recentId // Duplicate of recent
        let globalRow1 = createSearchRow(accountId: globalId1, username: Username(value: "alice.01"))

        let globalId2 = contactId // Duplicate of contact
        let globalRow2 = createSearchRow(accountId: globalId2, username: Username(value: "bob.01"))

        let globalId3 = try Data.randomOrError(of: 32)
        let globalRow3 = createSearchRow(accountId: globalId3, username: Username(value: "charlie"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [recentRow],
            contacts: [contactRow],
            global: .loaded([globalRow1, globalRow2, globalRow3]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.count == 1)
        #expect(result.global.rows[0].username == Username(value: "charlie"))
    }

    // MARK: - Excluding Set Tests

    @Test("Excluding set removes from all sections")
    func excludingSetRemovesFromAllSections() throws {
        let excludedId = try Data.randomOrError(of: 32)

        let recentRow = createSearchRow(accountId: excludedId, username: Username(value: "alice"))
        let contactRow = createSearchRow(accountId: excludedId, username: Username(value: "bob"))
        let globalRow = createSearchRow(accountId: excludedId, username: Username(value: "charlie"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [recentRow],
            contacts: [contactRow],
            global: .loaded([globalRow]),
            excluding: [excludedId],
            maxRecent: 5
        )

        #expect(result.recent.isEmpty)
        #expect(result.contacts.isEmpty)
        #expect(result.global.rows.isEmpty)
    }

    // MARK: - Sorting Tests

    @Test("Contacts sorted by username alphabetically")
    func contactsSortedByUsername() {
        let charlie = createSearchRow(username: Username(value: "charlie"))
        let alice = createSearchRow(username: Username(value: "alice"))
        let bob = createSearchRow(username: Username(value: "bob"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [charlie, alice, bob],
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.contacts.count == 3)
        #expect(result.contacts[0].username == Username(value: "alice"))
        #expect(result.contacts[1].username == Username(value: "bob"))
        #expect(result.contacts[2].username == Username(value: "charlie"))
    }

    @Test("Nil usernames sorted last in contacts")
    func nilUsernamesSortedLastInContacts() {
        let alice = createSearchRow(username: Username(value: "alice"))
        let noUsername1 = createSearchRow(username: nil)
        let bob = createSearchRow(username: Username(value: "bob"))
        let noUsername2 = createSearchRow(username: nil)

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [alice, noUsername1, bob, noUsername2],
            global: .loaded([]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.contacts.count == 4)
        #expect(result.contacts[0].username == Username(value: "alice"))
        #expect(result.contacts[1].username == Username(value: "bob"))
        #expect(result.contacts[2].username == nil)
        #expect(result.contacts[3].username == nil)
    }

    @Test("Global section sorted by username alphabetically")
    func globalSortedByUsername() {
        let charlie = createSearchRow(username: Username(value: "charlie"))
        let alice = createSearchRow(username: Username(value: "alice"))
        let bob = createSearchRow(username: Username(value: "bob"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: .loaded([charlie, alice, bob]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.count == 3)
        #expect(result.global.rows[0].username == Username(value: "alice"))
        #expect(result.global.rows[1].username == Username(value: "bob"))
        #expect(result.global.rows[2].username == Username(value: "charlie"))
    }

    @Test("Nil usernames sorted last in global")
    func nilUsernamesSortedLastInGlobal() {
        let alice = createSearchRow(username: Username(value: "alice"))
        let noUsername = createSearchRow(username: nil)
        let bob = createSearchRow(username: Username(value: "bob"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: .loaded([alice, noUsername, bob]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.count == 3)
        #expect(result.global.rows[0].username == Username(value: "alice"))
        #expect(result.global.rows[1].username == Username(value: "bob"))
        #expect(result.global.rows[2].username == nil)
    }

    // MARK: - Global Section Tests

    @Test("Global section empty when query is nil")
    func globalEmptyWhenQueryNil() {
        let globalRow = createSearchRow(username: Username(value: "alice"))

        let result = AccountSearchComposer.compose(
            query: nil,
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: .loaded([globalRow]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.isEmpty)
    }

    @Test("Global section empty when query is empty string")
    func globalEmptyWhenQueryEmpty() {
        let globalRow = createSearchRow(username: Username(value: "alice"))

        let result = AccountSearchComposer.compose(
            query: "",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: .loaded([globalRow]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.isEmpty)
    }

    @Test("Global section populated with non-empty query")
    func globalPopulatedWithNonEmptyQuery() {
        let globalRow = createSearchRow(username: Username(value: "alice"))

        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: .loaded([globalRow]),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.global.rows.count == 1)
    }

    // MARK: - Complex Integration Tests

    @Test("Complete workflow with all sections")
    func completeWorkflow() throws {
        let recentId = try Data.randomOrError(of: 32)
        let contactId = try Data.randomOrError(of: 32)
        let globalId = try Data.randomOrError(of: 32)

        let recent = [
            createSearchRow(
                accountId: recentId,
                username: Username(value: "alice"),
                matchTerms: ["alice"]
            )
        ]
        let contacts = [
            createSearchRow(accountId: contactId, username: Username(value: "bob"))
        ]
        let global = [
            createSearchRow(accountId: globalId, username: Username(value: "charlie"))
        ]

        let result = AccountSearchComposer.compose(
            query: "a",
            recent: recent,
            contacts: contacts,
            global: .loaded(global),
            excluding: [],
            maxRecent: 5
        )

        #expect(result.recent.count == 1)
        #expect(result.contacts.count == 1)
        #expect(result.global.rows.count == 1)
    }

    @Test("maxRecent respects custom limit")
    func maxRecentCustomLimit() {
        let recent = createSearchRows(count: 10, hasUsername: true)

        let result = AccountSearchComposer.compose(
            query: nil,
            recent: recent,
            contacts: [SearchRow<Int>](),
            global: .loaded([]),
            excluding: [],
            maxRecent: 2
        )

        #expect(result.recent.count == 2)
    }

    // MARK: - Global Outcome Tests

    @Test("Global outcome defaults to loaded")
    func globalOutcomeDefaultsToLoaded() {
        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            excluding: []
        )

        #expect(result.global.isLoaded)
        #expect(result.global.rows.isEmpty)
    }

    @Test(
        "Global outcome passes through",
        arguments: [AccountSearchGlobal<SearchRow<Int>>.pending([]), .loaded([]), .failed]
    )
    func globalOutcomePassesThrough(outcome: AccountSearchGlobal<SearchRow<Int>>) {
        let result = AccountSearchComposer.compose(
            query: "search",
            recent: [SearchRow<Int>](),
            contacts: [SearchRow<Int>](),
            global: outcome,
            excluding: []
        )

        #expect(result.global.isPending == outcome.isPending)
        #expect(result.global.hasFailed == outcome.hasFailed)
        #expect(result.global.rows.isEmpty)
    }
}

// MARK: - Helpers

private func createSearchRow(
    accountId: AccountId? = nil,
    username: Username? = Username(value: "user"),
    matchTerms: [String] = ["user"],
    payload: Int = 0
) -> SearchRow<Int> {
    let id = accountId ?? makeTestAccountId()
    return SearchRow(
        accountId: id,
        username: username,
        matchTerms: matchTerms,
        payload: payload
    )
}

private func makeTestAccountId() -> AccountId {
    var data = Data(repeating: 0, count: 32)
    if let randomData = try? Data.randomOrError(of: 32) {
        data = randomData
    }
    return data
}

private func createSearchRows(count: Int, hasUsername: Bool) -> [SearchRow<Int>] {
    (0 ..< count).map { index in
        let username = hasUsername ? Username(value: "user\(index)") : nil
        return createSearchRow(
            accountId: makeTestAccountId(),
            username: username,
            matchTerms: ["user\(index)"],
            payload: index
        )
    }
}
