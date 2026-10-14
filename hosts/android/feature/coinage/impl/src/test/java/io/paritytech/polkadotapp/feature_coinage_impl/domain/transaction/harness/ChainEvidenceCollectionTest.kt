package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.AliasRead
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery.ChainPresence
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The three-valued read contract, asserted on the evidence itself rather than through a verdict.
 *
 * Every rule is a positive comparison against one of these values, so getting the mapping wrong does not
 * produce an error — it silently disables or silently fires a rule. Both directions have happened here:
 * a voucher's silence read as ABSENT failed entries that had executed, and a coin's absence read as UNKNOWN
 * would disable every rule that turns on a coin being gone.
 */
class ChainEvidenceCollectionTest {
    @Test
    fun `a coin the chain holds is PRESENT`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val evidence = evidenceFor(coinEntry())

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[coinKeyOf(COIN_A)])
    }

    /**
     * A coin the chain holds nothing for is ABSENT, not UNKNOWN. Reading it as unknown disables Rules 3, 5
     * and 6, the only things that can fail an entry on its own assets.
     */
    @Test
    fun `a coin the chain holds nothing for is ABSENT, not UNKNOWN`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val evidence = evidenceFor(coinEntry())

        assertEquals(ChainPresence.ABSENT, evidence.presenceAtFinalized[coinKeyOf(COIN_B)])
    }

    @Test
    fun `a coin read that failed is UNKNOWN`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val entry = coinEntry()
        makeCoinsUnreadable(COIN_A)

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.UNKNOWN, evidence.presenceAtFinalized[coinKeyOf(COIN_A)])
        assertEquals(ChainPresence.UNKNOWN, evidence.presenceAtFinalized[coinKeyOf(COIN_B)])
    }

    /** Membership plus a resolved position is the voucher's existence signal; no alias entry means unspent. */
    @Test
    fun `a voucher in a ring with no alias entry is PRESENT and NOT_UNLOADED`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val evidence = evidenceFor(voucherEntry())

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.NOT_UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    @Test
    fun `a voucher whose alias is unloaded is PRESENT and UNLOADED`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val entry = voucherEntry()
        unloadVoucherOnChain(VOUCHER, finality = FINALIZED)

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNLOADED, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    /** The two axes are independent now: existence comes from `Members`, consumption from the alias. */
    @Test
    fun `a voucher whose alias read failed stays PRESENT with an unknown alias`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val entry = voucherEntry()
        makeVoucherAliasesUnreadable(VOUCHER)

        val evidence = evidenceFor(entry)

        assertEquals(ChainPresence.PRESENT, evidence.presenceAtFinalized[voucherKeyOf(VOUCHER)])
        assertEquals(AliasRead.UNKNOWN, evidence.aliasAtFinalized[voucherKeyOf(VOUCHER)])
    }

    private suspend fun DurabilityHarness.coinEntry() =
        givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

    private suspend fun DurabilityHarness.voucherEntry(): CoinageTransactionId {
        val id = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Voucher(testKey(VOUCHER))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        ).getOrThrow()
        releaseSubmissions()

        return id
    }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val VOUCHER = 5
