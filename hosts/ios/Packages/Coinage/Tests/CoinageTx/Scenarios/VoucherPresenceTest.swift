import Foundation
import Testing
@testable import Coinage

/// Voucher evidence taken from the chain rather than from the locally cached row.
///
/// A voucher's existence is its recycler membership; its consumption is the alias at the ring the
/// membership names. Member is PRESENT whatever its ring position, a non-member is UNKNOWN (never absent), and a
/// voucher
/// that is present but whose alias cannot be located — Suspended, or a failed alias read — is present
/// with alias UNKNOWN, which a two-valued flag could not express.
@Suite("Voucher Presence")
struct VoucherPresenceTest {
    private let voucher: CoinageKeyIndex = 5
    private let coinOut: CoinageKeyIndex = 2
    private let denomination = 3
    private let ring = 9
    private let otherRing = 7
    private let onboardingVoucher: CoinageKeyIndex = 6

    @Test("a healthy voucher in a ring is present and not unloaded")
    func healthyVoucherPresent() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)

        let evidence = try await harness.evidence(for: unloadEntry(harness))

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .present)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .notUnloaded)
    }

    @Test("an onboarding voucher is present and provably not unloaded")
    func onboardingVoucherPresent() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherOnboarding(voucher, denomination: denomination, finality: .finalized)

        let evidence = try await harness.evidence(for: unloadEntry(harness))

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .present)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .notUnloaded)
    }

    @Test("an alias under a ring the voucher is not in is not its alias")
    func aliasUnderOtherRingIgnored() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)
        harness.unloadVoucherAtOtherRing(voucher, denomination: denomination, ring: otherRing, finality: .finalized)

        let evidence = try await harness.evidence(for: unloadEntry(harness))

        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .notUnloaded)
    }

    @Test("an archived voucher is unknown on both maps")
    func archivedVoucherUnknown() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)
        let id = try await unloadEntry(harness)
        harness.archiveRecyclerOf(voucher, finality: .finalized)

        let evidence = try await harness.evidence(for: id)

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
    }

    @Test("a voucher suspended from its ring is present but says nothing about being unloaded")
    func suspendedVoucherUnknownAlias() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherSuspended(voucher, denomination: denomination, finality: .finalized)

        let evidence = try await harness.evidence(for: unloadEntry(harness))

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .present)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
    }

    @Test("a failed alias read does not erase what an onboarding voucher already proves")
    func failedAliasDoesNotEraseOnboarding() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherOnboarding(onboardingVoucher, denomination: denomination, finality: .finalized)
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)

        let id = try await harness.submit([harness.registration(
            inputs: [harness.voucherInput(onboardingVoucher), harness.voucherInput(voucher)],
            outputs: [harness.coinOutput(coinOut)],
            period: harnessMortalPeriod
        )])[0]
        await harness.releaseSubmissions()
        harness.makeVoucherAliasesUnreadable(voucher)

        let evidence = try await harness.evidence(for: id)

        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(onboardingVoucher)] == .notUnloaded)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
    }

    @Test("a failed read of the recycler a voucher belongs to leaves it unknown, not gone")
    func failedMembershipReadUnknown() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)
        let id = try await unloadEntry(harness)
        harness.makeRecyclerMembershipsUnreadable()

        let evidence = try await harness.evidence(for: id)

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
    }

    @Test("a failed read of a voucher's place in its ring leaves it unknown, not gone")
    func failedRingPositionReadUnknown() async throws {
        let harness = DurabilityHarness()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)
        let id = try await unloadEntry(harness)
        harness.makeRingPositionsUnreadable()

        let evidence = try await harness.evidence(for: id)

        #expect(evidence.presenceAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
        #expect(evidence.aliasAtFinalized[HarnessKeys.voucherMemberKey(voucher)] == .unknown)
    }

    @Test("Rule 4 fails an entry whose onboarding voucher outlived its mortality")
    func rule4FailsOnboardingPastMortality() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherOnboarding(voucher, denomination: denomination, finality: .finalized)
        let id = try await unloadEntry(harness)

        harness.makeCoinsUnreadable(coinOut)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("a failed voucher read leaves the unload undecided without stopping the pass")
    func failedVoucherReadLeavesUndecided() async throws {
        let harness = DurabilityHarness()
        harness.disableFallbackTxSearch()
        harness.givenVoucherInRecycler(voucher, denomination: denomination, ring: ring, finality: .finalized)
        let id = try await unloadEntry(harness)

        harness.makeRecyclerMembershipsUnreadable()
        harness.makeCoinsUnreadable(coinOut)
        try await harness.chainReachesMortalityOf(id, finality: .finalized)
        await harness.runPass()

        #expect(try await harness.status(of: id) == .pending)
    }

    private func unloadEntry(_ harness: DurabilityHarness) async throws -> CoinageTxId {
        let id = try await harness.registerVoucherUnload(vouchers: [voucher], outputCoin: coinOut)
        await harness.releaseSubmissions()
        return id
    }
}
