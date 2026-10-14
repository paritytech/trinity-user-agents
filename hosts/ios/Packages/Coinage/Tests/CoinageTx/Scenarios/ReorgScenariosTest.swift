import Foundation
import Testing
@testable import Coinage

/// Everything that turns on the best chain being rewritten under an entry.
///
/// A reorg is the one event that can retract evidence the subsystem has already acted on, so every
/// verdict resting on a block above the finalized head has to survive losing that block. The rule tests
/// pin Rule 0's canonicality clauses against hand-built evidence; these drive the same paths through a
/// real block tree, where a retracted block also takes its state and its dispatch outcome with it.
@Suite("Reorg Scenarios")
struct ReorgScenariosTest {
    private let coinA: CoinageKeyIndex = 1
    private let coinB: CoinageKeyIndex = 2
    private let coinC: CoinageKeyIndex = 3

    @Test("Rule 0 clause 1 clears the record and demotes when the recorded block is gone")
    func rule0ClearsRecordWhenBlockGone() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .inBest)
        #expect(try await harness.status(of: id) == .pendingSuccess)
        #expect(try await harness.entry(id)?.successDetectedAt != nil)

        harness.reorgLastBlocks(1)
        harness.advanceBlocks(1, finality: .inBest)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.entry(id)?.successDetectedAt == nil)
    }

    @Test("an output reorged out becomes nonexistent and its consumer fails on its own window")
    func outputReorgedOutFailsMinterOnWindow() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        harness.mintCoinsOnChain([coinB], finality: .inBest)
        await harness.runPass()
        #expect(try await harness.status(of: minter) == .pendingSuccess)

        harness.reorgLastBlocks(1)
        harness.advanceBlocks(1, finality: .inBest)
        await harness.runPass()
        #expect(try await harness.status(of: minter) == .pending)

        try await harness.chainReachesMortalityOf(minter, finality: .finalized)
        await harness.runPass()
        #expect(try await harness.status(of: minter) == .failure)
        #expect(try await harness.assetState(coin: coinB).minterStatus == .failure)
    }

    @Test("an unreachable node does not withdraw a success that was already detected")
    func unreachableNodeKeepsDetectedSuccess() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .inBest)
        let recorded = try #require(try await harness.entry(id)?.successDetectedAt)

        harness.advanceBlocks(2, finality: .finalized)
        harness.makeBlocksUnreadable(recorded.number)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pendingSuccess)
        #expect(try await harness.entry(id)?.successDetectedAt == recorded)
    }

    @Test("a reorg that shortens the chain past a recorded block clears the record")
    func reorgShorterThanRecordClearsRecord() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .inBest)
        let recorded = try #require(try await harness.entry(id)?.successDetectedAt)

        let depth = try #require(harness.chain.reorgDepths.last)
        harness.reorgLastBlocks(depth)
        #expect(harness.chain.bestHead.number < recorded.number)
        await harness.runPass()

        #expect(try await harness.entry(id)?.successDetectedAt == nil)
        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("an outcome is read from the same block the extrinsic was found in")
    func outcomeReadFromSameBlock() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)

        // The same transaction applied in two blocks with opposite outcomes: only the canonical one
        // counts, so a block reordered out must not lend its outcome to the other.
        let orphaned = harness.includeInBlock(txHash: txHash, success: false, finality: .inBest)
        harness.reorgLastBlocks(1)
        harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        // Blinded so no presence rule can decide first and the entry has to reach the search.
        harness.makeCoinsUnreadable(coinA, coinB)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(harness.chain.blockAt(hash: orphaned.hash)?.state.outcomes[txHash] == false)
        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }

    @Test("a retracted mint fails both the entry that minted the coin and the entry spending it")
    func retractedMintFailsBoth() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenEntryExecutedOnChain(inputCoin: coinA, outputCoin: coinB, finality: .inBest)
        await harness.runPass()
        #expect(try await harness.status(of: minter) == .pendingSuccess)

        let consumer = try await harness.givenUnwatchedEntry(inputCoin: coinB, outputCoin: coinC)

        harness.reorgLastBlocks(1)
        harness.advanceBlocks(1, finality: .inBest)
        await harness.runPass()
        #expect(try await harness.status(of: minter) == .pending)
        #expect(try await harness.status(of: consumer) == .pending)

        try await harness.chainReachesMortalityOf(consumer, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: minter) == .failure)
        #expect(try await harness.status(of: consumer) == .failure)
    }

    @Test("a head reorged between passes does not hold up a transaction finalized below it")
    func headReorgedDoesNotHoldUpFinalizedTx() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenEntryExecutedOnChain(inputCoin: coinA, outputCoin: coinB, finality: .inBest)
        let txBlock = harness.chain.bestHead.number

        harness.advanceBlocks(1, finality: .inBest)
        await harness.runPass()
        #expect(try await harness.status(of: id) == .pendingSuccess)

        // Only the head above the transaction's block is rewritten, so that block finalizes with the rest.
        harness.reorgLastBlocks(1)
        harness.advanceBlocks(1, finality: .finalized)
        harness.advanceBlocks(1, finality: .inBest)
        await harness.runPass()

        #expect(txBlock <= harness.chain.finalizedHead.number)
        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }
}
