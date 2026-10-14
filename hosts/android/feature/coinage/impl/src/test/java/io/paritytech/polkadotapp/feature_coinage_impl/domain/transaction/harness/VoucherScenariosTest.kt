package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The voucher half of `§ Testing`, which the coin scenarios cannot reach.
 *
 * A voucher is the only asset with positive consumption proof: its recycler alias reads as unloaded. A coin
 * has nothing equivalent — its absence is the strongest signal there is — so every rule that turns on
 * `provenConsumedOnChain` or `provenNotConsumed` is exercised here and nowhere else.
 */
class VoucherScenariosTest {
    @Test
    fun `a voucher still in its recycler after mortality fails the entry`() = scenario {
        disableFallbackTxSearch()
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val id = registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_B).getOrThrow()
        releaseSubmissions()

        // Blinded so the absent coin output cannot decide it first; the voucher is then the only evidence.
        makeCoinsUnreadable(COIN_B)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(id))
    }

    @Test
    fun `a voucher whose ring is archived mid-unload is not spent and decides nothing`() = scenario {
        disableFallbackTxSearch()
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val id = registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_B).getOrThrow()
        releaseSubmissions()

        // Archival takes the membership, so the voucher is gone with no unload proof. For a coin that
        // absence would be consumption; for a voucher it is ring cleaning, so it proves nothing either way.
        archiveRecyclerOf(VOUCHER, finality = FINALIZED)
        makeCoinsUnreadable(COIN_B)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `a voucher held by a live entry is not selectable`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)

        registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_B).getOrThrow()

        val state = repository.getAssetState(OwnAsset.Voucher(testKey(VOUCHER))).getOrThrow()
        assertTrue("a live consumer is what makes it unselectable", state.consumerStatus!!.isLive)
    }

    /**
     * Isolating alias evidence takes some care: the coin output would let Rule 3 decide, and the body search
     * would decide on its own once the window closed. With both silenced, the alias is the only evidence
     * left — and an unreadable one must decide nothing.
     */
    @Test
    fun `an unreadable alias is not an unloaded one and decides nothing`() = scenario {
        disableFallbackTxSearch()
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val id = registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_B).getOrThrow()
        releaseSubmissions()

        chainReachesMortalityOf(id, finality = FINALIZED)
        makeVoucherAliasesUnreadable(VOUCHER)
        makeCoinsUnreadable(COIN_B)
        runPass()

        assertEquals(PENDING, statusOf(id))
        assertTrue(repository.getAssetState(OwnAsset.Voucher(testKey(VOUCHER))).getOrThrow().consumerStatus!!.isLive)
    }

    @Test
    fun `the same entry finalizes once that alias reads as unloaded`() = scenario {
        disableFallbackTxSearch()
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        val id = registerVoucherUnload(voucher = VOUCHER, outputCoin = COIN_B).getOrThrow()
        releaseSubmissions()

        // Identical to the test above but for the alias, so the alias is what the difference measures.
        unloadVoucherOnChain(VOUCHER, finality = FINALIZED)
        chainReachesMortalityOf(id, finality = FINALIZED)
        makeCoinsUnreadable(COIN_B)
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    @Test
    fun `a voucher minted as an output is tracked as onboarding until its entry resolves`() = scenario {
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        mintCoinsOnChain(COIN_A, finality = FINALIZED)

        val id = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Voucher(testKey(NEW_VOUCHER))),
            groupId = null,
        ).getOrThrow()

        val state = repository.getAssetState(OwnAsset.Voucher(testKey(NEW_VOUCHER))).getOrThrow()
        assertEquals(PENDING, state.minterStatus)
        assertFalse(state.handedOff)
        assertEquals(PENDING, statusOf(id))
    }

    /**
     * The spec gives a voucher no existence condition (§ Model → Assets → Existence): archival may drop one
     * from `RecyclersCoinToRecycler` while it stays usable, and a voucher that was never minted has no alias
     * entry either — indistinguishable from a healthy one. Its silence is therefore UNKNOWN, never ABSENT.
     *
     * Everything that could decide this entry for another reason is silenced, so what is left is Rule 3
     * reading the voucher output. While a voucher's silence was ABSENT, that alone failed the entry.
     */
    @Test
    fun `a voucher output that says nothing does not fail its minter`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        givenVoucherInRecycler(voucher = NEW_VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)
        // Archived: the membership is gone, so the voucher can say nothing either way.
        archiveRecyclerOf(NEW_VOUCHER, finality = FINALIZED)

        val id = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Voucher(testKey(NEW_VOUCHER))),
            groupId = null,
        ).getOrThrow()
        releaseSubmissions()

        // The input is gone, so Rule 4 has nothing to say, and the search cannot reach its blocks.
        consumeCoinOnChain(COIN_A, finality = IN_BEST)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(id))
    }

    /** The same scenario with a coin output. Absence is evidence for a coin, so this one does fail. */
    @Test
    fun `a coin output that reads absent does fail its minter`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        consumeCoinOnChain(COIN_A, finality = IN_BEST)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(id))
    }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val VOUCHER = 5
private const val NEW_VOUCHER = 11
