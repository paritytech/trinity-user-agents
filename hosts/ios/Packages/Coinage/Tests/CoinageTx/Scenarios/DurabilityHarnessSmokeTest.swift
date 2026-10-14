import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

/// Proves the harness drives the real subsystem rather than a model of it: registration, the ledger
/// lock a registration takes, what a crash drops and what it must not, and one full recovery to a
/// terminal verdict.
@Suite("Durability Harness Smoke")
struct DurabilityHarnessSmokeTest {
    private let spentCoin: CoinageKeyIndex = 1
    private let mintedCoin: CoinageKeyIndex = 2
    private let voucher: CoinageKeyIndex = 5
    private let unknownCoin: CoinageKeyIndex = 99

    @Test("registration locks its input and takes submission ownership")
    func registrationLocksInputAndTakesOwnership() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)

        let id = try await harness.register(inputCoin: spentCoin, outputCoin: mintedCoin)

        #expect(try await harness.assetState(coin: spentCoin).consumerStatus == .pending)
        #expect(harness.isOwnedBySubmission(id))
    }

    @Test("a crash keeps the ledger and drops submission ownership")
    func crashKeepsLedgerDropsOwnership() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)
        let id = try await harness.register(inputCoin: spentCoin, outputCoin: mintedCoin)

        harness.crash()

        #expect(try await harness.status(of: id) == .pending)
        #expect(!harness.isOwnedBySubmission(id))
    }

    @Test("an uncommitted handoff is released on relaunch and a committed one is not")
    func handoffReleaseOnRelaunch() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)

        _ = try await harness.preCommitHandoff([harness.coinOutput(spentCoin)])
        #expect(try await harness.handoffKeys().contains(HarnessKeys.coinKey(spentCoin)))

        try await harness.relaunch()
        #expect(try await harness.handoffKeys().isEmpty)

        let commit = try await harness.preCommitHandoff([harness.coinOutput(spentCoin)])
        try commit.commit(in: InMemoryRegistrationScope())
        try await harness.relaunch()
        #expect(try await harness.handoffKeys().contains(HarnessKeys.coinKey(spentCoin)))
    }

    @Test("an output present at the finalized head finalizes the entry")
    func outputPresentFinalizes() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: spentCoin, outputCoin: mintedCoin)
        #expect(!harness.isOwnedBySubmission(id))
        #expect(harness.recoveryRequestCount > 0)

        harness.mintCoinsOnChain([mintedCoin], finality: .inBest)
        harness.finalizeToBest()
        await harness.runPass()

        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }

    @Test("an entry whose reads all fail keeps its status and its lock")
    func failedReadsKeepStatusAndLock() async throws {
        let harness = DurabilityHarness()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: spentCoin, outputCoin: mintedCoin)

        harness.makeCoinsUnreadable(spentCoin, mintedCoin)
        harness.advanceBlocks(1, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.assetState(coin: spentCoin).consumerStatus == .pending)
    }

    @Test("a voucher unload registers and a pass leaves it pending until executed")
    func voucherEntryPendingUntilExecuted() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([spentCoin], finality: .finalized)
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)

        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: mintedCoin)
        await harness.releaseSubmissions()

        harness.advanceBlocks(1, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("an untracked asset has no state")
    func untrackedAssetHasNoState() async throws {
        let harness = DurabilityHarness()

        let state = try await harness.assetState(coin: unknownCoin)

        #expect(state.minterStatus == nil)
        #expect(state.consumerStatus == nil)
        #expect(!state.handedOff)
    }
}
