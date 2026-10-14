import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

/// The rules, driven through the pass and the watcher.
///
/// `CoinageRulesTests` pins the ladder itself against hand-built evidence; these run the same behaviours
/// through evidence collection, the DAG and the compare-and-set write, which the pure tests cannot.
@Suite("Rule Scenarios")
struct RuleScenariosTest {
    private let coinA: CoinageKeyIndex = 1
    private let coinB: CoinageKeyIndex = 2
    private let coinC: CoinageKeyIndex = 3
    private let coinD: CoinageKeyIndex = 4
    private let coinSeed: CoinageKeyIndex = 9

    @Test("an entry whose output a peer claims before finality does not fall back to pending")
    func peerClaimBeforeFinalityKeepsRecord() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .inBest)

        // The peer takes and spends it: the output is gone and our input is consumed, but the recorded
        // block is still canonical, so the record is what holds the verdict up.
        #expect(await harness.handOff([harness.coinOutput(coinB)]))
        harness.consumeCoinOnChain(coinB, finality: .inBest)
        harness.consumeCoinOnChain(coinA, finality: .inBest)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pendingSuccess)
    }

    /// Asserted as a transition rather than a non-event: the same evidence that leaves the entry pending
    /// inside the window fails it outside, so a pass that silently skipped the entry could not produce both.
    @Test("Rules 3 and 4 do not fire before mortality has expired but do after")
    func rules3And4FireOnlyAfterMortality() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        // Output absent and input still available — both would fail it, but only past the window.
        harness.advanceBlocks(4, finality: .finalized)
        await harness.runPass()
        #expect(try await harness.status(of: id) == .pending)

        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()
        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("a coin the app did not mint itself never proves a spend, however it disappears")
    func unmintedCoinNeverProvesSpend() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        // The input is gone at the best head but the entry was never included; absence alone is not success.
        harness.consumeCoinOnChain(coinA, finality: .inBest)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.assetState(coin: coinB).minterStatus == .pending)
    }

    /// The handoff clause of `ownCoinInputs` has no reachable scenario: the two invariants that surround
    /// it make an entry's input and a handoff mark mutually exclusive, in both orders. `CoinageRulesTests`
    /// covers the clause directly; what is testable here is the pair of refusals that keep it unreachable.
    @Test("an input and a handoff mark are mutually exclusive in both orders")
    func inputAndHandoffMarkAreExclusive() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA, coinC], finality: .finalized)
        try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        await #expect(throws: CoinageTxError.handoffOfClaimedAsset(HarnessKeys.coinKey(coinA).toHex())) {
            try await harness.preCommitHandoff([harness.coinOutput(coinA)])
        }

        #expect(await harness.handOff([harness.coinOutput(coinC)]))
        await #expect(throws: CoinageTxError.inputHandedOff(HarnessKeys.coinKey(coinC).toHex())) {
            try await harness.register(inputCoin: coinC, outputCoin: coinD)
        }
    }

    @Test("ownCoinInputs declines while the input minter's own window is still open")
    func ownCoinInputsDeclineWhileMinterWindowOpen() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinSeed], finality: .finalized)
        try await harness.givenEntryDecided(inputCoin: coinSeed, outputCoin: coinA, finality: .finalized)

        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        harness.consumeCoinOnChain(coinA, finality: .inBest)
        harness.finalizeToBest()
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("a partially read window leaves the entry pending and the next pass re-reads it")
    func partialWindowRereadNextPass() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)

        let block = harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        blindPresence(harness)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)

        harness.makeBlocksUnreadable(block.number)
        await harness.runPass()
        #expect(try await harness.status(of: id) == .pending)

        // The search carries nothing between passes, so the block becoming readable is all it needs.
        makeBlocksReadable(harness)
        await harness.runPass()
        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }

    @Test("the search fails an entry whose dispatch failed in a finalized block")
    func searchFailsOnFailedDispatch() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)

        harness.includeInBlock(txHash: txHash, success: false, finality: .inBest)
        blindPresence(harness)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("the watcher fails an entry finalized with a failed dispatch")
    func watcherFailsOnFinalizedFailedDispatch() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: false, finality: .inBest)
        events.finalized(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("the watcher records no successDetectedAt for a block whose dispatch failed")
    func watcherRecordsNothingForFailedDispatch() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let (id, txHash, events) = try await watchedEntry(harness)

        let block = harness.includeInBlock(txHash: txHash, success: false, finality: .inBest)
        events.inBlock(block.hash, txHash: txHash)
        await harness.releaseSubmissions()

        #expect(try await harness.entry(id)?.successDetectedAt == nil)
        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("an unreadable outcome leaves the entry pending and its reservations held on every pass")
    func unreadableOutcomeHoldsReservations() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)

        harness.includeInBlock(txHash: txHash, success: true, finality: .inBest)
        blindPresence(harness)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        makeOutcomeUnreadable(harness, txHash: txHash)

        for _ in 0 ..< 3 {
            await harness.runPass()
            #expect(try await harness.status(of: id) == .pending)
            #expect(try await harness.assetState(coin: coinA).consumerStatus == .pending)
        }
    }

    @Test("a failed read never satisfies absent, so Rules 5 and 6 cannot fire")
    func failedReadNeverSatisfiesAbsent() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinSeed], finality: .finalized)
        let minter = try await harness.givenEntryDecided(inputCoin: coinSeed, outputCoin: coinA, finality: .finalized)
        try await harness.chainReachesMortalityOf(minter, finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        // Unreadable rather than absent: the inputs would otherwise look consumed.
        harness.makeCoinsUnreadable(coinA)
        harness.advanceBlocks(1, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("an input never has two claimants that are not failures")
    func inputNeverHasTwoLiveClaimants() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let first = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)
        #expect(try await nonFailedClaimants(harness, of: coinA) == 1)

        try await harness.chainReachesMortalityOf(first, finality: .finalized)
        await harness.runPass()
        #expect(try await harness.status(of: first) == .failure)
        #expect(try await nonFailedClaimants(harness, of: coinA) == 0)

        try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinC)
        #expect(try await nonFailedClaimants(harness, of: coinA) == 1)
    }

    /// Tx A consumes C1 and mints C2, and executes on chain. Our own later transaction B then spends C2,
    /// and also executes, while the app is offline. The app comes back after A's mortality has passed,
    /// with B still unresolved. A must not be failed: C2 is gone, but B is what took it.
    ///
    /// Rule 3 is the one that would fail A here, and its live-consumer guard is what holds it off. Without
    /// the guard A is written failure terminally for a transaction that succeeded, and C1 stops counting
    /// as claimed — so the coin becomes registrable again and can be spent twice.
    @Test("a live consumer of an output keeps Rule 3 from failing the entry that minted it")
    func liveConsumerGuardsMinter() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenEntryExecutedOnChain(
            inputCoin: coinA,
            outputCoin: coinB,
            finality: .finalized
        )
        try await harness.givenUnwatchedEntry(inputCoin: coinB, outputCoin: coinC)

        // Our own consumer spent it, so the output is absent while the minter's input is consumed too and
        // Rule 4 has nothing to say. The only thing between the minter and failure is the guard.
        harness.consumeCoinOnChain(coinB, finality: .finalized)
        try await harness.chainReachesMortalityOf(minter, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: minter) == .pending)
    }

    /// The same chain state, except no transaction of ours ever claimed C2. Nothing we know of could have
    /// taken it, so its absence does mean A never ran.
    @Test("the same absent output with no consumer does fail its minter")
    func absentOutputWithoutConsumerFailsMinter() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenEntryExecutedOnChain(
            inputCoin: coinA,
            outputCoin: coinB,
            finality: .finalized
        )

        harness.consumeCoinOnChain(coinB, finality: .finalized)
        try await harness.chainReachesMortalityOf(minter, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: minter) == .failure)
    }

    /// The user moves a coin out to an external asset. Tx A consumes C2 and produces nothing the app can
    /// look for on chain. The app is offline while it executes, and C2 is gone by the time it looks. A must
    /// resolve as success.
    ///
    /// C2 was minted by a transaction that finalized and whose window has closed, so C2 certainly existed
    /// and was certainly visible. Nobody else holds its key, so A is the only thing that can have taken it.
    /// That is Rule 5, and it is the only way an operation with no trackable output is decided from state.
    @Test("a coin moved out to an external asset resolves as success from the coin being gone")
    func offboardResolvesFromCoinGone() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .finalized)
        try await harness.chainReachesMortalityOf(minter, finality: .finalized)

        let offboard = try await givenUnwatchedOffboard(harness, inputCoin: coinB)
        harness.consumeCoinOnChain(coinB, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: offboard) == .finalizedSuccess)
    }

    /// The same offboard, read while the block that consumed the coin is still only on the best chain. C2
    /// is gone there but still present at the finalized head. A is a success, but not one the chain has
    /// settled.
    ///
    /// Rule 6 is the unfinalized twin of Rule 5: it keeps the entry out of the way without claiming more
    /// than the chain has committed to.
    @Test("the same spend seen only on the best chain is a success that is not yet final")
    func offboardSeenOnlyAtBestIsPendingSuccess() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let minter = try await harness.givenEntryDecided(inputCoin: coinA, outputCoin: coinB, finality: .finalized)
        try await harness.chainReachesMortalityOf(minter, finality: .finalized)

        let offboard = try await givenUnwatchedOffboard(harness, inputCoin: coinB)
        harness.consumeCoinOnChain(coinB, finality: .inBest)
        await harness.runPass()

        #expect(try await harness.status(of: offboard) == .pendingSuccess)
    }

    /// A payment never reaches a block and the app stays closed past its mortality. On reopening, the node
    /// answers for the best head but not for the finalized one. The payment must be failed and its coin
    /// returned, not held until the connection improves.
    ///
    /// Best-head evidence is a reason to wait inside the window and none outside it, where the transaction
    /// can no longer execute. The window guard on Rule 4 is what drops the entry through to the search,
    /// the only thing left that can decide it.
    @Test("a payment that never landed is still failed when only the best head can be read")
    func neverLandedFailsWithOnlyBestHeadReadable() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        // The best head has to sit above the finalized one, or blinding one blinds both and the best head
        // has nothing to say either.
        harness.advanceBlocks(1, finality: .inBest)
        makeCoinsUnreadableAtFinalizedHead(harness)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .failure)
        // The coin is the user's to spend again.
        #expect(try await harness.assetState(coin: coinA).consumerStatus == nil)
    }
}

// MARK: - Helpers

private extension RuleScenariosTest {
    /// Makes every coin read fail, so no rule that needs presence can fire and the ladder reaches the search.
    func blindPresence(_ harness: DurabilityHarness) {
        harness.makeCoinsUnreadable(coinA, coinB, coinC, coinSeed)
    }

    func makeBlocksReadable(_ harness: DurabilityHarness) {
        harness.chainFactory.faults.unreadableBlocks = []
        harness.chainFactory.faults.everyBlockUnreadable = false
    }

    /// The extrinsic is found, but the events that say whether its dispatch succeeded are not.
    func makeOutcomeUnreadable(_ harness: DurabilityHarness, txHash: Data) {
        harness.chainFactory.faults.unreadableOutcomes.insert(txHash)
    }

    /// Coin reads fail at the finalized head but still answer at the best one. Anchored to whatever is
    /// finalized when this is called, so a scenario moves the chain into place first.
    func makeCoinsUnreadableAtFinalizedHead(_ harness: DurabilityHarness) {
        harness.stateReader.faults.statelessBlocks.insert(harness.chain.finalizedHead.hash)
    }

    /// A coin in and nothing trackable out — the offboard shape, with its watcher released.
    func givenUnwatchedOffboard(_ harness: DurabilityHarness, inputCoin: CoinageKeyIndex) async throws -> CoinageTxId {
        let id = try await harness.registerOffboard(inputCoin: inputCoin)
        await harness.releaseSubmissions()
        return id
    }

    func nonFailedClaimants(_ harness: DurabilityHarness, of coin: CoinageKeyIndex) async throws -> Int {
        let key = HarnessKeys.coinKey(coin)
        return try await harness.store.getAllEntries()
            .filter { $0.status != .failure }
            .count { entry in entry.inputs.contains { $0.publicKey == key } }
    }

    func watchedEntry(
        _ harness: DurabilityHarness
    ) async throws -> (id: CoinageTxId, txHash: Data, events: HarnessStatusStream) {
        let events = HarnessStatusStream()
        harness.submissionStatuses = { _ in events }
        let id = try await harness.register(inputCoin: coinA, outputCoin: coinB)
        let txHash = try #require(try await harness.entry(id)?.txHash)
        return (id, txHash, events)
    }
}
