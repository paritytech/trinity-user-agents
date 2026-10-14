import DurableTransactionsTestSupport
import ExtrinsicServiceExt
import Foundation
import Testing
@testable import Coinage

/// What the watcher writes, as opposed to what a pass derives.
///
/// The watcher is the only writer of `successDetectedAt` before finality, and that record is what keeps
/// an entry's outputs selectable in the window between inclusion and finalization. Everything here is a
/// status the node reports, so the stream is driven directly rather than through the chain.
@Suite("Watcher Scenarios")
struct WatcherScenariosTest {
    private let coinA: CoinageKeyIndex = 1
    private let coinB: CoinageKeyIndex = 2
    private let coinC: CoinageKeyIndex = 3
    private let coinD: CoinageKeyIndex = 4

    @Test("an inclusion with a successful dispatch records the block it was seen in")
    func inBlockRecordsBlock() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        events.inBlock(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pendingSuccess)
        #expect(try await harness.entry(id)?.successDetectedAt?.number == block.number)
    }

    @Test("a finalized block with a successful dispatch finalizes the entry")
    func finalizedFinalizes() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .finalized)
        events.finalized(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }

    @Test("a transaction the node refuses before submission is failed without waiting for its window")
    func preSubmissionRejectionFailsImmediately() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, _, events) = try await watchedEntry(harness)

        events.failedToSubmit(PreSubmissionValidationFailedError())
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .failure)
        #expect(harness.submissionCount == 1)
    }

    @Test("a submission that failed after reaching the node is left to the pass")
    func postSubmissionFailureLeftToPass() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, _, events) = try await watchedEntry(harness)

        events.failedToSubmit(HarnessSubscriptionLost())
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("a transaction that finalized asks for no recovery")
    func finalizedAsksNoRecovery() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .finalized)
        events.finalized(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .finalizedSuccess)
        #expect(harness.recoveryRequestCount == 0)
    }

    @Test("a transaction refused before submission asks for no recovery")
    func preSubmissionRejectionAsksNoRecovery() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, _, events) = try await watchedEntry(harness)

        events.failedToSubmit(PreSubmissionValidationFailedError())
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .failure)
        #expect(harness.recoveryRequestCount == 0)
    }

    @Test("a transaction left undecided when its watch ends is handed to recovery")
    func undecidedHandedToRecovery() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        events.dropped(txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pending)
        #expect(harness.recoveryRequestCount > 0)
    }

    @Test("an inclusion in a block that cannot be read records nothing")
    func inclusionInUnreadableBlockRecordsNothing() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        harness.makeBlocksUnreadable(block.number)
        events.inBlock(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.entry(id)?.successDetectedAt == nil)
    }

    @Test("a retraction of the recorded block clears the record")
    func retractionOfRecordedBlockClears() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        events.inBlock(block.hash, txHash: txHash)
        events.retracted(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.entry(id)?.successDetectedAt == nil)
    }

    @Test("a retraction naming another block leaves the record alone")
    func retractionOfOtherBlockLeavesRecord() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        harness.advanceBlocks(1, finality: .inBest)

        events.inBlock(block.hash, txHash: txHash)
        events.retracted(harness.chain.bestHead.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pendingSuccess)
        #expect(try await harness.entry(id)?.successDetectedAt?.number == block.number)
    }

    @Test("a Ready arriving after an inclusion does not wipe the record")
    func readyAfterInclusionKeepsRecord() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        events.inBlock(block.hash, txHash: txHash)
        events.ready(txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pendingSuccess)
        #expect(try await harness.entry(id)?.successDetectedAt != nil)
    }

    @Test("a subscription that fails hands the entry to recovery with its lock intact")
    func failedSubscriptionHandsToRecovery() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        harness.submissionStatuses = { _ in HarnessStatusStream.failing() }

        let id = try await harness.register(inputCoin: coinA, outputCoin: coinB)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .pending)
        #expect(!harness.isOwnedBySubmission(id))
        #expect(harness.recoveryRequestCount > 0)
        #expect(try await harness.assetState(coin: coinA).consumerStatus == .pending)
    }

    @Test("a failed subscription does not take down the watcher of another transaction")
    func failedSubscriptionDoesNotAffectAnother() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA, coinC], finality: .finalized)

        let events = HarnessStatusStream()
        harness.submissionStatuses = { index in index == 0 ? .failing() : events }

        let failed = try await harness.register(inputCoin: coinA, outputCoin: coinB)
        let healthy = try await harness.register(inputCoin: coinC, outputCoin: coinD)
        let txHash = try #require(try await harness.entry(healthy)?.txHash)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .finalized)
        events.finalized(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: healthy) == .finalizedSuccess)
        #expect(try await harness.status(of: failed) == .pending)
    }

    private func watchedEntry(
        _ harness: DurabilityHarness
    ) async throws -> (id: CoinageTxId, txHash: Data, events: HarnessStatusStream) {
        let events = HarnessStatusStream()
        harness.submissionStatuses = { _ in events }
        let id = try await harness.register(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)
        return (id, txHash, events)
    }
}
