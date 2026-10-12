@testable import polkadot_app
import Foundation
import Operation_iOS
import SubstrateSdk

final class MockRemoteContactOperationFactory: RemoteContactOperationMaking {
    var searchResult: [Chat.RemoteContact] = []
    var fetchResult: Chat.RemoteContact?

    var searchError: Error?
    var fetchError: Error?

    /// When set, `fetch(by:)` suspends until the test opens the gate.
    var fetchGate: RemoteFetchGate?

    // Recorded inputs
    var receivedSearchQuery: String?
    var receivedFetchAccountId: AccountId?

    func search(by query: String) -> CompoundOperationWrapper<[Chat.RemoteContact]> {
        receivedSearchQuery = query

        let operation = ClosureOperation<[Chat.RemoteContact]> {
            if let error = self.searchError {
                throw error
            }
            return self.searchResult
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        receivedFetchAccountId = accountId

        await fetchGate?.waitUntilOpened()

        if let error = fetchError {
            throw error
        }
        return fetchResult
    }
}

/// A handshake the test drives: the lookup announces that it started and then suspends until the
/// test opens the gate, so a test can cancel while the lookup is in flight without timing guesses.
final class RemoteFetchGate {
    private let entered: AsyncStream<Void>
    private let enteredContinuation: AsyncStream<Void>.Continuation
    private let opened: AsyncStream<Void>
    private let openedContinuation: AsyncStream<Void>.Continuation

    init() {
        let enteredStream = AsyncStream<Void>.makeStream()
        entered = enteredStream.stream
        enteredContinuation = enteredStream.continuation

        let openedStream = AsyncStream<Void>.makeStream()
        opened = openedStream.stream
        openedContinuation = openedStream.continuation
    }

    func waitUntilOpened() async {
        enteredContinuation.yield()

        var iterator = opened.makeAsyncIterator()
        _ = await iterator.next()
    }

    func waitUntilEntered() async {
        var iterator = entered.makeAsyncIterator()
        _ = await iterator.next()
    }

    func open() {
        openedContinuation.finish()
    }
}
