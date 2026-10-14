package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageRegistrationError
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUBMISSION
import kotlinx.coroutines.flow.first
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AssetScenariosTest {
    // Leaves the search on: the mark's job is to stop Rule 3 preempting, and the search is then what proves
    // the entry actually succeeded. With it off the entry is merely PENDING, which does not say that.
    @Test
    fun `minting tx succeed even though its outputs has been consumed after handoff`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)

        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_B)))).getOrThrow().commit().getOrThrow()
        consumeCoinOnChain(COIN_B, finality = IN_BEST)
        // This makes windowClosed to be true and thus challenges noPotentialConsumers against handoff
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
        assertTrue(assetStateOf(COIN_B).handedOff)
    }

    /**
     * This is not a realistic state under all assumptions of the spec
     * Rather, it can happen only if the output coin is spent without app knowing that
     * The only realistic case right now is when the handoff mark got lost, e.g. after restoring from backup
     * So this test is here to document this behavior
     */
    @Test
    fun `minting entry without the handoff mark fails on its missing output`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)

        consumeCoinOnChain(COIN_B, finality = IN_BEST)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(id))
    }

    @Test
    fun `a finalized transaction is never rewritten by a later pass or a relaunch`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)
        assertEquals(FINALIZED_SUCCESS, assetStateOf(COIN_A).consumerStatus)

        advanceBlocks(3, finality = FINALIZED)
        runPass()
        crash()
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(id))
        assertEquals(FINALIZED_SUCCESS, assetStateOf(COIN_A).consumerStatus)
    }

    @Test
    fun `propagation writes nothing to a terminal entry`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        chainReachesMortalityOf(first, finality = FINALIZED)
        runPass()
        assertEquals(FAILURE, statusOf(first))

        // A successor finalizing afterwards must not un-fail it.
        val second = givenUnwatchedEntry(inputCoin = COIN_B, outputCoin = COIN_C)
        mintCoinsOnChain(COIN_C, finality = IN_BEST)
        finalizeToBest()
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(second))
        assertEquals(FAILURE, statusOf(first))
    }

    @Test
    fun `a chain of three promotes all three once the last one finalizes`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        val second = givenUnwatchedEntry(inputCoin = COIN_B, outputCoin = COIN_C)
        val third = givenUnwatchedEntry(inputCoin = COIN_C, outputCoin = COIN_E)

        mintCoinsOnChain(COIN_E, finality = IN_BEST)
        finalizeToBest()

        // Propagation reads a snapshot, so each pass promotes one hop; the loop is what runs it to a fixpoint.
        repeat(3) { runPass() }

        assertEquals(FINALIZED_SUCCESS, statusOf(third))
        assertEquals(FINALIZED_SUCCESS, statusOf(second))
        assertEquals(FINALIZED_SUCCESS, statusOf(first))
    }

    // Leaves the search on: it is what resolves the middle of a chain in one pass. Rule 3 cannot, because an
    // entry's output stays claimed until its own successor is FAILURE, so the rules alone walk one hop per pass.
    @Test
    fun `a chain whose first entry fails resolves every successor within one window`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        val second = givenUnwatchedEntry(inputCoin = COIN_B, outputCoin = COIN_C)
        val third = givenUnwatchedEntry(inputCoin = COIN_C, outputCoin = COIN_E)

        // One window past the last of them, not one window per hop.
        chainReachesMortalityOf(third, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(first))
        assertEquals(FAILURE, statusOf(second))
        assertEquals(FAILURE, statusOf(third))
    }

    /**
     * A chat payment is saved and its split is scheduled, but not built yet.
     * A second payment picks the same coin before the first one's extrinsic exists.
     * The second registration is refused.
     *
     * The lock is what scheduling buys: an input is claimed from the moment the payment is saved, not from
     * the moment its proofs are ready — otherwise every slow build would be a window for a double spend.
     */
    @Test
    fun `inputs of a transaction waiting to be built are refused to a second registration`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        givenSubmissionPolicy(PolicyBehaviour.HOLD)
        scheduleRetriable(inputCoin = COIN_A, COIN_B)

        val second = register(inputCoin = COIN_A, outputCoin = COIN_C)

        assertTrue(second.exceptionOrNull() is CoinageRegistrationError.InputAlreadyClaimed)
        assertEquals(PENDING_SUBMISSION, assetStateOf(COIN_A).consumerStatus)
    }

    @Test
    fun `a received key not yet on chain is registrable and a second claim is refused`() = scenario {
        // Deliberately never put on chain: a peer's transfer may not be included yet when we claim it.
        val peerKey = coinKeyOf(PEER_COIN)

        val first = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Coin.Received(peerKey)),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        )
        assertTrue(first.isSuccess)

        val second = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = listOf(CoinageInput.Coin.Received(peerKey)),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_C))),
            groupId = null,
        )

        assertTrue(second.exceptionOrNull() is CoinageRegistrationError.InputAlreadyClaimed)
    }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val COIN_C = 3
private const val COIN_E = 6
private const val PEER_COIN = 50
private const val VOUCHER = 5
