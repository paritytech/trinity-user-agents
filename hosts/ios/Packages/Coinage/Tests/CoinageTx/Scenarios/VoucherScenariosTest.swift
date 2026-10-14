import Foundation
import Testing
@testable import Coinage

/// The voucher half of the durability scenarios, which the coin scenarios cannot reach.
///
/// A voucher is the only asset with positive consumption proof: its recycler alias reads as unloaded. A
/// coin has nothing equivalent — its absence is the strongest signal there is — so every rule that turns
/// on proven consumption or proven-not-unloaded is exercised here. Several cases also pin the iOS↔Android
/// parity fix: an archived or otherwise silent voucher reads UNKNOWN, never absent, so it never fails an
/// entry the way a coin's absence would.
@Suite("Voucher Scenarios")
struct VoucherScenariosTest {
    private let coinA: CoinageKeyIndex = 1
    private let coinB: CoinageKeyIndex = 2
    private let voucher: CoinageKeyIndex = 5
    private let newVoucher: CoinageKeyIndex = 11

    @Test("a voucher still in its recycler after mortality fails the entry")
    func voucherStillInRecyclerFails() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)
        await harness.releaseSubmissions()

        harness.makeCoinsUnreadable(coinB)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("a voucher whose ring is archived mid-unload is not spent and decides nothing")
    func archivedMidUnloadDecidesNothing() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)
        await harness.releaseSubmissions()

        // Archival takes the membership: for a coin that absence is consumption; for a voucher it is ring
        // cleaning, so it proves nothing either way (parity fix — reads UNKNOWN, not absent).
        harness.archiveRecyclerOf(voucher, finality: .finalized)
        harness.makeCoinsUnreadable(coinB)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("a voucher held by a live entry is not selectable")
    func voucherHeldByLiveEntry() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)

        _ = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)

        let state = try await harness.assetState(voucher: voucher)
        #expect(state.consumerStatus?.isLive == true)
    }

    @Test("an unreadable alias is not an unloaded one and decides nothing")
    func unreadableAliasDecidesNothing() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)
        await harness.releaseSubmissions()

        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        harness.makeVoucherAliasesUnreadable(voucher)
        harness.makeCoinsUnreadable(coinB)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.assetState(voucher: voucher).consumerStatus?.isLive == true)
    }

    @Test("the same entry finalizes once that alias reads as unloaded")
    func finalizesOnceAliasUnloaded() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)
        await harness.releaseSubmissions()

        harness.unloadVoucherOnChain(voucher, finality: .finalized)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        harness.makeCoinsUnreadable(coinB)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .finalizedSuccess)
    }

    @Test("a voucher minted as an output is tracked as onboarding until its entry resolves")
    func mintedVoucherTrackedUntilResolved() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        harness.mintCoinsOnChain([coinA], finality: .finalized)

        let id = try await harness.registerVoucherMint(inputCoin: coinA, voucher: newVoucher)

        let state = try await harness.assetState(voucher: newVoucher)
        #expect(state.minterStatus == .pending)
        #expect(!state.handedOff)
        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("a voucher output that says nothing does not fail its minter")
    func silentVoucherOutputDoesNotFailMinter() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        harness.givenVoucherInRecycler(newVoucher, denomination: 3, ring: 7, finality: .finalized)
        harness.archiveRecyclerOf(newVoucher, finality: .finalized)

        let id = try await harness.registerVoucherMint(inputCoin: coinA, voucher: newVoucher)
        await harness.releaseSubmissions()

        harness.consumeCoinOnChain(coinA, finality: .inBest)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("an unload that succeeded then had its voucher suspended is not wrongly failed")
    func unloadedThenSuspendedNotFailed() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: 3, ring: 7, finality: .finalized)
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinB)
        await harness.releaseSubmissions()

        // The unload runs (alias written under ring 7), then a ring rebuild suspends the voucher — it
        // loses its ring index, so the alias can no longer be located. Before the fix, the voucher read
        // not-unloaded and Rule 4 failed an unload that actually succeeded. Now it reads unknown, so the
        // entry stays undecided rather than being wrongly failed.
        harness.unloadVoucherOnChain(voucher, finality: .finalized)
        harness.suspendVoucher(voucher, finality: .finalized)
        harness.makeCoinsUnreadable(coinB)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) != .failure)
        #expect(try await harness.status(of: id) == .pending)
    }

    @Test("a coin output that reads absent does fail its minter")
    func absentCoinOutputFailsMinter() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.mintCoinsOnChain([coinA], finality: .finalized)
        let id = try await harness.givenUnwatchedEntry(inputCoin: coinA, outputCoin: coinB)

        harness.consumeCoinOnChain(coinA, finality: .inBest)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .failure)
    }
}
