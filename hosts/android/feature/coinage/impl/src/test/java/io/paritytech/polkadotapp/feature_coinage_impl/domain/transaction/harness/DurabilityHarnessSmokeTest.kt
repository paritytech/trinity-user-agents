package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Proves the harness drives the real subsystem rather than a model of it: registration, the ledger lock a
 * registration takes, what a crash drops and what it must not, and one full recovery to a terminal verdict.
 */
class DurabilityHarnessSmokeTest {
    @Test
    fun `registration locks its input and takes submission ownership`() = scenario {
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)

        val id = register(SPENT_COIN, MINTED_COIN).getOrThrow()

        assertEquals(PENDING, assetStateOf(SPENT_COIN).consumerStatus)
        assertTrue(ownedEntries.isOwnedBySubmission(id))
    }

    @Test
    fun `a crash keeps the ledger and drops submission ownership`() = scenario {
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)
        val id = register(SPENT_COIN, MINTED_COIN).getOrThrow()

        crash()

        assertEquals(PENDING, statusOf(id))
        assertFalse(ownedEntries.isOwnedBySubmission(id))
    }

    @Test
    fun `an uncommitted handoff is released on relaunch and a committed one is not`() = scenario {
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)

        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(SPENT_COIN)))).getOrThrow()
        assertTrue(repository.getHandoffKeys().getOrThrow().contains(coinKeyOf(SPENT_COIN)))

        relaunch()
        assertTrue(repository.getHandoffKeys().getOrThrow().isEmpty())

        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(SPENT_COIN)))).getOrThrow().commit().getOrThrow()
        relaunch()
        assertTrue(repository.getHandoffKeys().getOrThrow().contains(coinKeyOf(SPENT_COIN)))
    }

    @Test
    fun `an output present at the finalized head finalizes the entry`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = SPENT_COIN, outputCoin = MINTED_COIN)
        assertFalse("a pass skips exactly what submission owns", ownedEntries.isOwnedBySubmission(id))
        assertRecoveryWasRequested()

        mintCoinsOnChain(MINTED_COIN, finality = IN_BEST)
        finalizeToBest()
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    @Test
    fun `an entry whose reads all fail keeps its status and its lock`() = scenario {
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = SPENT_COIN, outputCoin = MINTED_COIN)

        makeCoinsUnreadable(SPENT_COIN, MINTED_COIN)
        advanceBlocks(1, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(id))
        assertEquals(PENDING, assetStateOf(SPENT_COIN).consumerStatus)
    }

    @Test
    fun `a voucher entry registers and collects evidence without the native lib`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(SPENT_COIN, finality = FINALIZED)
        givenVoucherInRecycler(voucher = VOUCHER, denomination = 3, ring = 7, finality = FINALIZED)

        val id = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Voucher(testKey(VOUCHER))),
            outputs = listOf(OwnAsset.Coin(testKey(MINTED_COIN))),
            groupId = null,
        ).getOrThrow()
        releaseSubmissions()

        advanceBlocks(1, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(id))
        assertTrue("registration must derive the member key", voucherDerivation.memberKeyCalls > 0)
        assertTrue("evidence must reach the alias lookup", voucherDerivation.aliasCalls > 0)
    }

    @Test
    fun `an untracked asset has no state`() = scenario {
        val state = assetStateOf(UNKNOWN_COIN)

        assertNull(state.minterStatus)
        assertNull(state.consumerStatus)
        assertFalse(state.handedOff)
    }
}

private const val SPENT_COIN = 1
private const val MINTED_COIN = 2
private const val VOUCHER = 5
private const val UNKNOWN_COIN = 99
