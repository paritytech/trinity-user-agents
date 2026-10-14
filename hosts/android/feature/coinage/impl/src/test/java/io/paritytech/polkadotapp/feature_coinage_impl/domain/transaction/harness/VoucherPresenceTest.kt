package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.AliasRead
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.ChainPresence
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Voucher evidence taken from the chain rather than from the locally cached row.
 *
 * A voucher's existence is `Members`, reached through the denomination in `RecyclersCoinToRecycler`; its
 * consumption is the alias at the ring `Members` names. The local row's ring index comes from a best-block
 * subscription and can be stale, which is the one input that must not decide anything.
 */
class VoucherPresenceTest {
    @Test
    fun `a healthy voucher in a ring is PRESENT`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)

        val evidence = evidenceFor(unloadEntry())

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.NOT_UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /** No ring index means no unload was possible, so not-unloaded is inferred rather than read. */
    @Test
    fun `an onboarding voucher is PRESENT and provably NOT_UNLOADED`() = scenario {
        givenVoucherOnboarding(voucher = VOUCHER, denomination = DENOMINATION, finality = FINALIZED)

        val evidence = evidenceFor(unloadEntry())

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.NOT_UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /**
     * The alias is keyed by ring, so a read has to use the ring the chain says the voucher is in. An alias
     * sitting under any other ring belongs to a different membership and must not be picked up.
     */
    @Test
    fun `an alias under a ring the voucher is not in is not its alias`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)
        unloadVoucherAtOtherRing(voucher = VOUCHER, denomination = DENOMINATION, ring = OTHER_RING, finality = FINALIZED)

        val evidence = evidenceFor(unloadEntry())

        assertEquals(AliasRead.NOT_UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /** After archival the membership is gone, so nothing about the voucher can be established. */
    @Test
    fun `an archived voucher is UNKNOWN on both maps`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)
        val entry = unloadEntry()
        archiveRecyclerOf(VOUCHER, finality = FINALIZED)

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.UNKNOWN, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /** A voucher that was never in a ring cannot have been unloaded, so the entry demonstrably did not run. */
    @Test
    fun `Rule 4 fails an entry whose onboarding voucher outlived its mortality`() = scenario {
        disableFallbackTxSearch()
        givenVoucherOnboarding(voucher = VOUCHER, denomination = DENOMINATION, finality = FINALIZED)
        val id = unloadEntry()

        // The coin output is absent and untouched, so Rule 3 would fail the entry on it and the voucher
        // input would never be consulted. Blinded, the only thing left is Rule 4 reading the voucher.
        makeCoinsUnreadable(COIN_OUT)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(id))
    }

    /**
     * An onboarding voucher's not-unloaded is inferred from its position, so a failed alias read — which
     * happens here because the entry also holds a voucher that does need one — must not erase it.
     */
    @Test
    fun `a failed alias read does not erase what an onboarding voucher already proves`() = scenario {
        givenVoucherOnboarding(voucher = ONBOARDING_VOUCHER, denomination = DENOMINATION, finality = FINALIZED)
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)

        val id = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Voucher(testKey(ONBOARDING_VOUCHER)), CoinageInput.Voucher(testKey(VOUCHER))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_OUT))),
            groupId = null,
        ).getOrThrow()
        releaseSubmissions()
        // Only the in-ring voucher has an alias to silence; the onboarding one has no key at all.
        makeVoucherAliasesUnreadable(VOUCHER)

        val evidence = evidenceFor(id)

        assertEquals(AliasRead.NOT_UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(ONBOARDING_VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /**
     * A voucher is suspended from its ring while an unload of it is in flight.
     * It is still a member of the recycler, so it exists.
     * It holds no ring index, and the alias that would say whether it was unloaded is keyed by that index.
     * Nothing may be concluded either way.
     *
     * A wrong "unloaded" would finalize an unload that never ran; a wrong "not unloaded" would leave the
     * voucher offered to the user and fail an unload that did.
     */
    @Test
    fun `a voucher suspended from its ring exists but says nothing about being unloaded`() = scenario {
        givenVoucherSuspended(voucher = VOUCHER, denomination = DENOMINATION, finality = FINALIZED)

        val evidence = evidenceFor(unloadEntry())

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /**
     * A voucher sits in its recycler with an unload of it in flight.
     * The node then fails to answer for the recycler it belongs to — the ordinary transient RPC error.
     * The voucher must read as unknown, not as gone.
     *
     * Silence from a failed read is not the chain saying the voucher is not there. Reading it as absence
     * would make an unload still perfectly able to run look like one that already did, or never could.
     */
    @Test
    fun `a failed read of the recycler a voucher belongs to leaves it unknown, not gone`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)
        val entry = unloadEntry()
        makeRecyclerMembershipsUnreadable()

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.UNKNOWN, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /** The same for the ring membership itself, which is the other half of a voucher's existence. */
    @Test
    fun `a failed read of a voucher's place in its ring leaves it unknown, not gone`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)
        val entry = unloadEntry()
        makeRingPositionsUnreadable()

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.UNKNOWN, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /**
     * The user unloads a voucher and the node fails to answer for the recycler it belongs to.
     * The unload has run past its mortality, so it is exactly the entry a pass would otherwise fail.
     * It must stay undecided, and the pass must finish rather than abandon the ledger over a read error.
     *
     * The coin output is blinded too, so the failed voucher read is the only thing left that could decide it.
     */
    @Test
    fun `a failed voucher read leaves the unload undecided without stopping the pass`() = scenario {
        disableFallbackTxSearch()
        givenVoucherInRecycler(voucher = VOUCHER, denomination = DENOMINATION, ring = RING, finality = FINALIZED)
        val unload = unloadEntry()

        makeRecyclerMembershipsUnreadable()
        makeCoinsUnreadable(COIN_OUT)
        chainReachesMortalityOf(unload, finality = FINALIZED)

        assertTrue("a read error must not abort the pass", recoveryPass.run().isSuccess)
        assertEquals(PENDING, statusOf(unload))
    }

    private suspend fun DurabilityHarness.unloadEntry(): CoinageTransactionId {
        val id = registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_OUT).getOrThrow()
        releaseSubmissions()

        return id
    }
}

private const val VOUCHER = 5
private const val COIN_OUT = 2
private const val DENOMINATION = 3
private const val RING = 9
private const val OTHER_RING = 7
private const val ONBOARDING_VOUCHER = 6
