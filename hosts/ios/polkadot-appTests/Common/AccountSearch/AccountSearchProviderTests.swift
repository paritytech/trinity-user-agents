import Testing
import SubstrateSdk
import Foundation
import AsyncExtensions
import Operation_iOS
import SDKLogger
import NovaCrypto
@testable import polkadot_app

struct AccountSearchProviderTests {
    // MARK: - Routing: empty/nil query

    @Test("Empty query uses allContacts() and skips global search")
    func emptyQueryUsesAllContacts() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: "")

        #expect(localSearch.didRequestAllContacts)
        #expect(localSearch.receivedUsernamePrefix == nil)
        #expect(localSearch.receivedAccountId == nil)
        #expect(remoteSearch.receivedSearchQuery == nil)
        #expect(remoteSearch.receivedFetchAccountId == nil)
    }

    @Test("Nil query uses allContacts() and skips global search")
    func nilQueryUsesAllContacts() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: nil)

        #expect(localSearch.didRequestAllContacts)
        #expect(localSearch.receivedUsernamePrefix == nil)
        #expect(localSearch.receivedAccountId == nil)
        #expect(remoteSearch.receivedSearchQuery == nil)
    }

    // MARK: - Routing: non-address query

    @Test("Non-address query uses searchContacts() and global search()")
    func nonAddressQueryUsesSearchContacts() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: "alice")

        #expect(localSearch.receivedUsernamePrefix == "alice")
        #expect(localSearch.receivedAccountId == nil)
        #expect(!localSearch.didRequestAllContacts)
        #expect(remoteSearch.receivedSearchQuery == "alice")
    }

    // MARK: - Routing: valid SS58 address

    @Test("Valid SS58 address uses contact() and fetch(), not searchContacts()")
    func validAddressUsesContactAndFetch() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)
        let targetAccountId = try Data.randomOrError(of: 32)
        let address = try SS58AddressFactory().address(fromAccountId: targetAccountId, type: 0)

        // Set fetchResult so fetch(by:) succeeds and search() is not called as fallback
        remoteSearch.fetchResult = try makeRemoteContact(accountId: targetAccountId, username: "found")

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: address)

        #expect(localSearch.receivedAccountId == targetAccountId)
        #expect(localSearch.receivedUsernamePrefix == nil)
        #expect(!localSearch.didRequestAllContacts)
        #expect(remoteSearch.receivedFetchAccountId == targetAccountId)
        #expect(remoteSearch.receivedSearchQuery == nil)
    }

    // MARK: - String normalization: trimmingDot

    @Test("Query with .dot suffix is trimmed before lookup")
    func trimmingDotRemovesSuffix() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: "alice.dot")

        #expect(localSearch.receivedUsernamePrefix == "alice")
        #expect(remoteSearch.receivedSearchQuery == "alice")
    }

    @Test("Interior .dot is preserved when normalizing the query")
    func trimmingDotPreservesInteriorMatch() async throws {
        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        _ = try await collectPhases(from: provider, query: "alice.dotty")

        #expect(localSearch.receivedUsernamePrefix == "alice.dotty")
        #expect(remoteSearch.receivedSearchQuery == "alice.dotty")
    }

    // MARK: - Filtering: blocked contacts

    @Test("Blocked contacts are filtered from results")
    func blockedContactsFiltered() async throws {
        let blockedContact = try makeContact(
            accountId: Data.randomOrError(of: 32),
            username: "blocked_user",
            isBlocked: true
        )
        let allowedContact = try makeContact(
            accountId: Data.randomOrError(of: 32),
            username: "allowed_user",
            isBlocked: false
        )

        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [blockedContact, allowedContact]

        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        let phases = try await collectPhases(from: provider, query: nil)
        let result = try #require(phases.last)

        #expect(result.contacts.count == 1)
        #expect(result.contacts[0].username?.value == "allowed_user")
    }

    @Test("Blocked account returned only by remote search is excluded from global")
    func blockedAccountFromRemoteSearchExcluded() async throws {
        let blockedAccountId = try Data.randomOrError(of: 32)
        let blockedContact = makeContact(
            accountId: blockedAccountId,
            username: "blocked_remote",
            isBlocked: true
        )

        // The blocked contact is known locally (so blockedContacts() returns it)
        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [blockedContact]

        // Remote search returns the same blocked contact
        let remoteContactWithBlockedId = try makeRemoteContact(
            accountId: blockedAccountId,
            username: "blocked_remote"
        )

        let remoteSearch = MockRemoteContactOperationFactory()
        remoteSearch.searchResult = [remoteContactWithBlockedId]

        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        // Search with a query that won't match "blocked_remote" locally
        let phases = try await collectPhases(from: provider, query: "xyz")
        let result = try #require(phases.last)

        // Should not contain the blocked account in global results
        #expect(result.global.rows.allSatisfy { $0.accountId != blockedAccountId })
    }

    // MARK: - Filtering: own account id

    @Test("Own account id is excluded from results")
    func ownAccountIdExcluded() async throws {
        let ownAccountId = try Data.randomOrError(of: 32)
        let otherAccountId = try Data.randomOrError(of: 32)

        let ownContact = makeContact(accountId: ownAccountId, username: "own")
        let otherContact = makeContact(accountId: otherAccountId, username: "other")

        let localSearch = MockLocalContactSearch()
        localSearch.contacts = [ownContact, otherContact]

        let remoteSearch = MockRemoteContactOperationFactory()

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        let phases = try await collectPhases(from: provider, query: nil)
        let result = try #require(phases.last)

        #expect(result.contacts.count == 1)
        #expect(result.contacts[0].username?.value == "other")
    }

    // MARK: - Failure handling: global search failure

    @Test("Global search failure is tolerated, returns contacts without global section")
    func globalSearchFailureNotPropagated() async throws {
        let localSearch = MockLocalContactSearch()
        let contact = try makeContact(accountId: Data.randomOrError(of: 32), username: "local_user")
        localSearch.contacts = [contact]

        let remoteSearch = MockRemoteContactOperationFactory()
        remoteSearch.searchError = AccountSearchTestError.lookupFailed

        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        let phases = try await collectPhases(from: provider, query: "search")
        let result = try #require(phases.last)

        #expect(result.contacts.count == 1)
        #expect(result.global.rows.isEmpty)
    }

    // MARK: - Recents stream updates

    @Test("Recents stream updates trigger sourcesChanged and appear in results")
    func recentsStreamUpdatesAppear() async throws {
        let (recentsStream, continuation) = AsyncStream<[SearchRow<Int>]>.makeStream()

        let recentId = try Data.randomOrError(of: 32)
        let recentRow = SearchRow(
            accountId: recentId,
            username: Username(value: "recent_user"),
            matchTerms: ["recent_user"],
            payload: 0
        )

        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { recentsStream.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        var sourcesChangedIterator = provider.sourcesChanged().makeAsyncIterator()
        provider.setup()

        continuation.yield([])
        _ = try await sourcesChangedIterator.next()

        continuation.yield([recentRow])
        _ = try await sourcesChangedIterator.next()

        let phases = try await collectPhases(from: provider, query: nil)
        let result = try #require(phases.last)

        #expect(result.recent.count == 1)
        #expect(result.recent[0].username?.value == "recent_user")

        continuation.finish()
    }

    // MARK: - Remote contact fetch by id

    @Test("Remote contact fetch by valid address returns in global section")
    func remoteContactFetchByAddressReturns() async throws {
        let targetAccountId = try Data.randomOrError(of: 32)
        let address = try SS58AddressFactory().address(fromAccountId: targetAccountId, type: 0)

        let remoteContact = try makeRemoteContact(
            accountId: targetAccountId,
            username: "remote_user"
        )

        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        remoteSearch.fetchResult = remoteContact

        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        let phases = try await collectPhases(from: provider, query: address)
        let result = try #require(phases.last)

        #expect(result.global.rows.count == 1)
        #expect(result.global.rows[0].username?.value == "remote_user")
    }

    // MARK: - Global search returns results

    @Test("Global search with non-address query returns results in global section")
    func globalSearchReturnsResults() async throws {
        let remoteContact = try makeRemoteContact(
            accountId: Data.randomOrError(of: 32),
            username: "search_result"
        )

        let localSearch = MockLocalContactSearch()
        let remoteSearch = MockRemoteContactOperationFactory()
        remoteSearch.searchResult = [remoteContact]

        let ownAccountId = try Data.randomOrError(of: 32)

        let provider: AccountSearchProvider<Int> = AccountSearchProvider(
            recentRowsStream: { AsyncStream<[SearchRow<Int>]> { _ in }.eraseToAnyAsyncSequence() },
            localContactSearch: localSearch,
            remoteContactSearch: remoteSearch,
            ownAccountId: ownAccountId,
            logger: MockLogger()
        )
        provider.setup()

        let phases = try await collectPhases(from: provider, query: "search")
        let result = try #require(phases.last)

        #expect(result.global.rows.count == 1)
        #expect(result.global.rows[0].username?.value == "search_result")
    }

    // MARK: - Phased search

    @Test("Non-empty query yields a pending phase and then a loaded phase")
    func queryYieldsPendingThenLoadedPhase() async throws {
        let context = try await makeQueryContext()
        let phases = try await collectPhases(from: context.provider, query: "alice")

        #expect(phases.count == 2)

        let pending = try #require(phases.first)
        #expect(pending.global.isPending)
        #expect(pending.recent.count == 1)
        #expect(pending.contacts.count == 1)
        #expect(pending.global.rows.isEmpty)

        let loaded = try #require(phases.last)
        #expect(loaded.global.isLoaded)
        #expect(loaded.recent.count == 1)
        #expect(loaded.contacts.count == 1)
        #expect(loaded.global.rows.count == 1)
    }

    @Test("Global failure yields a failed phase keeping recent and contacts")
    func globalFailureYieldsFailedPhase() async throws {
        let context = try await makeQueryContext(globalFails: true)
        let phases = try await collectPhases(from: context.provider, query: "alice")

        #expect(phases.count == 2)

        let failed = try #require(phases.last)
        #expect(failed.global.hasFailed)
        #expect(failed.global.rows.isEmpty)
        #expect(failed.recent.count == 1)
        #expect(failed.contacts.count == 1)
    }

    @Test("Empty query yields a single loaded phase without global results")
    func emptyQueryYieldsSingleLoadedPhase() async throws {
        let context = try await makeQueryContext()
        let phases = try await collectPhases(from: context.provider, query: "")

        #expect(phases.count == 1)
        #expect(phases[0].global.isLoaded)
        #expect(phases[0].global.rows.isEmpty)
        #expect(context.remoteSearch.receivedSearchQuery == nil)
    }

    @Test("Nil query yields a single loaded phase without global results")
    func nilQueryYieldsSingleLoadedPhase() async throws {
        let context = try await makeQueryContext()
        let phases = try await collectPhases(from: context.provider, query: nil)

        #expect(phases.count == 1)
        #expect(phases[0].global.isLoaded)
        #expect(phases[0].global.rows.isEmpty)
        #expect(context.remoteSearch.receivedSearchQuery == nil)
    }

    @Test("Local contacts failure finishes the stream with the thrown error")
    func contactsFailureFinishesStreamWithError() async throws {
        let context = try await makeQueryContext()
        context.localSearch.contactsError = AccountSearchTestError.lookupFailed

        await #expect(throws: (any Error).self) {
            _ = try await collectPhases(from: context.provider, query: "alice")
        }
    }

    @Test("Blocked contacts failure finishes the stream with the thrown error")
    func blockedFailureFinishesStreamWithError() async throws {
        let context = try await makeQueryContext()
        context.localSearch.blockedContactsError = AccountSearchTestError.lookupFailed

        await #expect(throws: (any Error).self) {
            _ = try await collectPhases(from: context.provider, query: "alice")
        }
    }

    // MARK: - Preserved global rows across restarts

    @Test("Restarting the same query keeps the previous global rows in the pending phase")
    func sameQueryRestartKeepsGlobalRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        let phases = try await collectPhases(from: context.provider, query: "alice")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.count == 1)
        #expect(pending.global.rows[0].username?.value == "alice_remote")
    }

    @Test("Restarting with a different query yields an empty global in the pending phase")
    func differentQueryRestartEmptiesGlobalRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        let phases = try await collectPhases(from: context.provider, query: "bob")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.isEmpty)
    }

    @Test("A failed global lookup clears the preserved rows")
    func globalFailureClearsPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        context.remoteSearch.searchError = AccountSearchTestError.lookupFailed
        let failedPhases = try await collectPhases(from: context.provider, query: "alice")
        let failed = try #require(failedPhases.last)

        #expect(failed.global.hasFailed)

        let phases = try await collectPhases(from: context.provider, query: "alice")
        let pending = try #require(phases.first)

        #expect(pending.global.rows.isEmpty)
    }

    @Test("The loaded phase replaces the preserved rows instead of appending to them")
    func loadedPhaseReplacesPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        context.remoteSearch.searchResult = try [
            makeRemoteContact(accountId: Data.randomOrError(of: 32), username: "alice_remote_2")
        ]

        let phases = try await collectPhases(from: context.provider, query: "alice")
        let loaded = try #require(phases.last)

        #expect(loaded.global.isLoaded)
        #expect(loaded.global.rows.count == 1)
        #expect(loaded.global.rows[0].username?.value == "alice_remote_2")
    }

    @Test("Extending the query keeps the still-matching preserved rows")
    func extendedQueryKeepsMatchingPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        let phases = try await collectPhases(from: context.provider, query: "alice_r")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.count == 1)
        #expect(pending.global.rows[0].username?.value == "alice_remote")
    }

    @Test("Extending the query drops the preserved rows that no longer match")
    func extendedQueryDropsNonMatchingPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        let phases = try await collectPhases(from: context.provider, query: "alice_x")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.isEmpty)
    }

    @Test("Shortening the query keeps the preserved rows")
    func shortenedQueryKeepsPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice_rem")

        let phases = try await collectPhases(from: context.provider, query: "alice")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.count == 1)
        #expect(pending.global.rows[0].username?.value == "alice_remote")
    }

    @Test("An account id query yields an empty global in the pending phase despite cached rows")
    func accountIdQueryIgnoresPreservedRows() async throws {
        let context = try await makeQueryContext()
        _ = try await collectPhases(from: context.provider, query: "alice")

        let targetAccountId = try Data.randomOrError(of: 32)
        context.remoteSearch.fetchResult = try makeRemoteContact(accountId: targetAccountId, username: "alice_by_id")
        let address = try SS58AddressFactory().address(fromAccountId: targetAccountId, type: 0)

        let phases = try await collectPhases(from: context.provider, query: address)
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.isEmpty)
    }

    @Test("Rows cached for an unrelated query are not surfaced even when a row matches the new query")
    func unrelatedCachedQueryDoesNotSurfaceMatchingRows() async throws {
        let context = try await makeQueryContext()

        // Caches the "alice_remote" row under the unrelated query "bob".
        _ = try await collectPhases(from: context.provider, query: "bob")

        let phases = try await collectPhases(from: context.provider, query: "alice")
        let pending = try #require(phases.first)

        #expect(pending.global.isPending)
        #expect(pending.global.rows.isEmpty)
    }

    // MARK: - Cancellation

    @Test("A cancelled global lookup finishes after the pending phase without a failed phase")
    func cancelledGlobalLookupYieldsNoFailedPhase() async throws {
        let context = try await makeQueryContext()
        context.remoteSearch.fetchError = CancellationError()

        let address = try SS58AddressFactory().address(fromAccountId: Data.randomOrError(of: 32), type: 0)

        let phases = try await collectPhases(from: context.provider, query: address)

        #expect(phases.count == 1)
        #expect(!phases.contains { $0.global.hasFailed })

        let pending = try #require(phases.first)
        #expect(pending.global.isPending)
    }

    @Test("Cancelling while the global lookup is in flight yields no failed phase")
    func cancellingDuringGlobalLookupYieldsNoFailedPhase() async throws {
        let context = try await makeQueryContext()
        let gate = RemoteFetchGate()
        context.remoteSearch.fetchGate = gate
        context.remoteSearch.fetchError = AccountSearchTestError.lookupFailed

        let address = try SS58AddressFactory().address(fromAccountId: Data.randomOrError(of: 32), type: 0)

        let collector = Task { try await collectPhases(from: context.provider, query: address) }

        await gate.waitUntilEntered()
        collector.cancel()
        gate.open()

        let phases = try await collector.value

        #expect(phases.count <= 1)
        #expect(!phases.contains { $0.global.hasFailed })
    }
}

private enum AccountSearchTestError: Error {
    case lookupFailed
}

// MARK: - Phased search helpers

private struct QueryContext {
    let provider: AccountSearchProvider<Int>
    let localSearch: MockLocalContactSearch
    let remoteSearch: MockRemoteContactOperationFactory
}

/// Seeds one recent row, one local contact and one remote match, all matching the "alice" prefix.
private func makeQueryContext(globalFails: Bool = false) async throws -> QueryContext {
    let localSearch = MockLocalContactSearch()
    localSearch.contacts = try [makeContact(accountId: Data.randomOrError(of: 32), username: "alice_local")]

    let remoteSearch = MockRemoteContactOperationFactory()
    if globalFails {
        remoteSearch.searchError = AccountSearchTestError.lookupFailed
    } else {
        remoteSearch.searchResult = try [
            makeRemoteContact(accountId: Data.randomOrError(of: 32), username: "alice_remote")
        ]
    }

    let recentRow = try SearchRow(
        accountId: Data.randomOrError(of: 32),
        username: Username(value: "alice_recent"),
        matchTerms: ["alice_recent"],
        payload: 0
    )

    let (recentsStream, continuation) = AsyncStream<[SearchRow<Int>]>.makeStream()
    let provider = try AccountSearchProvider<Int>(
        recentRowsStream: { recentsStream.eraseToAnyAsyncSequence() },
        localContactSearch: localSearch,
        remoteContactSearch: remoteSearch,
        ownAccountId: Data.randomOrError(of: 32),
        logger: MockLogger()
    )

    var sourcesChangedIterator = provider.sourcesChanged().makeAsyncIterator()
    provider.setup()
    continuation.yield([recentRow])
    _ = try await sourcesChangedIterator.next()

    return QueryContext(provider: provider, localSearch: localSearch, remoteSearch: remoteSearch)
}

private func collectPhases(
    from provider: AccountSearchProvider<Int>,
    query: String?
) async throws -> [AccountSearchSections<Int, ContactSearchPayload>] {
    var phases: [AccountSearchSections<Int, ContactSearchPayload>] = []

    for try await phase in provider.searchPhases(query: query) {
        phases.append(phase)
    }

    return phases
}

// MARK: - Helpers

private func makeContact(
    accountId: AccountId = Data(),
    username: String = "test_user",
    isBlocked: Bool = false
) -> Chat.Contact {
    Chat.Contact(
        accountId: accountId,
        username: username,
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

private func makeRemoteContact(
    accountId: AccountId = Data(),
    username: String = "test_remote"
) throws -> Chat.RemoteContact {
    try Chat.RemoteContact(
        accountId: accountId,
        username: username,
        chatPublicKey: Chat.PublicKey(rawData: Data(repeating: 0, count: 32)),
        imageData: nil
    )
}
