package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageRegistrationError
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import kotlinx.coroutines.flow.MutableSharedFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `coinage-durability-spec.md § Testing`, *Rules* — all fifteen, driven through the pass and the watcher.
 *
 * `CoinageRulesTest` already pins the ladder itself against hand-built evidence; these run the same
 * behaviours through evidence collection, the DAG and the compare-and-set write, which the pure tests cannot.
 */
class RuleScenariosTest {
    @Test
    fun `an entry whose output a peer claims before finality does not fall back to PENDING`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)

        // The peer takes and spends it: the output is gone and our input is consumed, but the recorded
        // block is still canonical, so the record is what holds the verdict up.
        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_B)))).getOrThrow().commit().getOrThrow()
        consumeCoinOnChain(COIN_B, finality = IN_BEST)
        consumeCoinOnChain(COIN_A, finality = IN_BEST)
        runPass()

        assertEquals(PENDING_SUCCESS, statusOf(id))
    }

    /**
     * Asserted as a transition rather than a non-event: the same evidence that leaves the entry PENDING
     * inside the window fails it outside, so a pass that silently skipped the entry could not produce both.
     */

    /**
     * A retriable entry's window closes with its input still on chain, which proves it never ran.
     * The pass writes it back to its policy rather than failing it, and its input stays locked meanwhile.
     */
    @Test
    fun `a failure the pass proves is handed back to its policy instead of failing`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        givenSubmissionPolicy(PolicyBehaviour.HOLD)
        val id = registerRetriable(inputCoin = COIN_A, COIN_B)
        releaseSubmissions()

        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(DurableTxStatus.PENDING_SUBMISSION, statusOf(id))
        assertEquals(DurableTxStatus.PENDING_SUBMISSION, assetStateOf(COIN_A).consumerStatus)
    }

    @Test
    fun `Rules 3 and 4 do not fire before mortality has expired but do after`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        // Output absent and input still available — both would fail it, but only past the window.
        advanceBlocks(4, finality = FINALIZED)
        runPass()
        assertEquals(PENDING, statusOf(id))

        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()
        assertEquals(FAILURE, statusOf(id))
    }

    @Test
    fun `a coin the app did not mint itself never proves a spend, however it disappears`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        // The input is gone at the best head but the entry was never included; absence alone is not success.
        consumeCoinOnChain(COIN_A, finality = IN_BEST)
        runPass()

        assertEquals(PENDING, statusOf(id))
        assertEquals(PENDING, assetStateOf(COIN_B).minterStatus)
    }

    /**
     * The handoff clause of `ownCoinInputs` has no reachable scenario: the two invariants that surround it
     * make an entry's input and a handoff mark mutually exclusive, in both orders. `CoinageRulesTest` covers
     * the clause directly; what is testable here is the pair of refusals that keep it unreachable.
     */
    @Test
    fun `an input and a handoff mark are mutually exclusive in both orders`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_C, finality = FINALIZED)
        givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        val handoffAfterClaim = service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_A))))
        assertTrue(handoffAfterClaim.exceptionOrNull() is CoinageRegistrationError.HandoffOfClaimedAsset)

        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_C)))).getOrThrow().commit().getOrThrow()
        val claimAfterHandoff = register(COIN_C, COIN_D)
        assertTrue(claimAfterHandoff.exceptionOrNull() is CoinageRegistrationError.InputHandedOff)
    }

    @Test
    fun `ownCoinInputs declines while the input minter's own window is still open`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_SEED, finality = FINALIZED)
        givenEntryDecided(inputCoin = COIN_SEED, outputCoin = COIN_A, finality = FINALIZED)

        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        consumeCoinOnChain(COIN_A, finality = IN_BEST)
        finalizeToBest()
        runPass()

        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `a partially read window leaves the entry PENDING and the next pass re-reads it`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        val txHash = repository.getEntry(id).getOrThrow()!!.txHash

        val block = includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        blindPresence()
        chainReachesMortalityOf(id, finality = FINALIZED)

        makeBlocksUnreadable(block.number)
        runPass()
        assertEquals(PENDING, statusOf(id))

        // The search carries nothing between passes, so the block becoming readable is all it needs.
        makeBlocksReadable()
        runPass()
        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    @Test
    fun `the search fails an entry whose dispatch failed in a finalized block`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        includeInBlock(repository.getEntry(id).getOrThrow()!!.txHash, ExtrinsicOutcome.FAILURE, finality = IN_BEST)
        blindPresence()
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(id))
    }

    @Test
    fun `the watcher fails an entry finalized with a failed dispatch`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val events = statusEvents()
        submissionStatuses = { events }
        val id = register(COIN_A, COIN_B).getOrThrow()
        val txHash = repository.getEntry(id).getOrThrow()!!.txHash

        val block = includeInBlock(txHash, ExtrinsicOutcome.FAILURE, finality = IN_BEST)
        events.tryEmit(ExtrinsicStatus.Finalized(block.hash, txHash))
        releaseSubmissions()

        assertEquals(FAILURE, statusOf(id))
    }

    @Test
    fun `the watcher records no successDetectedAt for a block whose dispatch failed`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val events = statusEvents()
        submissionStatuses = { events }
        val id = register(COIN_A, COIN_B).getOrThrow()
        val txHash = repository.getEntry(id).getOrThrow()!!.txHash

        val block = includeInBlock(txHash, ExtrinsicOutcome.FAILURE, finality = IN_BEST)
        events.tryEmit(ExtrinsicStatus.InBlock(block.hash, txHash))
        releaseSubmissions()

        assertNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)
        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `an unreadable outcome leaves the entry PENDING and its reservations held on every pass`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        val txHash = repository.getEntry(id).getOrThrow()!!.txHash

        includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        blindPresence()
        chainReachesMortalityOf(id, finality = FINALIZED)
        makeOutcomeUnreadable(txHash)

        repeat(3) {
            runPass()
            assertEquals(PENDING, statusOf(id))
            assertEquals(PENDING, assetStateOf(COIN_A).consumerStatus)
        }
    }

    @Test
    fun `a failed read never satisfies absent, so Rules 5 and 6 cannot fire`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_SEED, finality = FINALIZED)
        val minter = givenEntryDecided(inputCoin = COIN_SEED, outputCoin = COIN_A, finality = FINALIZED)
        chainReachesMortalityOf(minter, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        // Unreadable rather than absent: the inputs would otherwise look consumed.
        makeCoinsUnreadable(COIN_A)
        advanceBlocks(1, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `an input never has two claimants that are not failures`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        assertEquals(1, nonFailedClaimantsOf(COIN_A))

        chainReachesMortalityOf(first, finality = FINALIZED)
        runPass()
        assertEquals(FAILURE, statusOf(first))
        assertEquals(0, nonFailedClaimantsOf(COIN_A))

        givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_C)
        assertEquals(1, nonFailedClaimantsOf(COIN_A))
    }

    /** Makes every coin read fail, so no rule that needs presence can fire and the ladder reaches the search. */

    /**
     * Tx A consumes C1 and mints C2, and executes on chain.
     * Our own later transaction B then spends C2, and also executes, while the app is offline.
     * The app comes back after A's mortality has passed, with B still unresolved.
     * A must not be failed: C2 is gone, but B is what took it.
     *
     * Rule 3 is the one that would fail A here, and its live-consumer guard is what holds it off. Without
     * the guard A is written FAILURE terminally for a transaction that succeeded, and C1 stops counting as
     * claimed — so the coin becomes registrable again and can be spent twice.
     */
    @Test
    fun `a live consumer of an output keeps Rule 3 from failing the entry that minted it`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val minter = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)
        givenUnwatchedEntry(inputCoin = COIN_B, outputCoin = COIN_C)

        // Our own consumer spent it, so the output is absent while the minter's input is consumed too and
        // Rule 4 has nothing to say. The only thing between the minter and FAILURE is the guard.
        consumeCoinOnChain(COIN_B, finality = FINALIZED)
        chainReachesMortalityOf(minter, finality = FINALIZED)
        runPass()

        assertEquals(PENDING, statusOf(minter))
    }

    /**
     * The same chain state, except no transaction of ours ever claimed C2.
     * Nothing we know of could have taken it, so its absence does mean A never ran.
     */
    @Test
    fun `the same absent output with no consumer does fail its minter`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val minter = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)

        consumeCoinOnChain(COIN_B, finality = FINALIZED)
        chainReachesMortalityOf(minter, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(minter))
    }

    /**
     * The user moves a coin out to an external asset.
     * Tx A consumes C2 and produces nothing the app can look for on chain.
     * The app is offline while it executes, and C2 is gone by the time it looks.
     * A must resolve as success.
     *
     * C2 was minted by a transaction that finalized and whose window has closed, so C2 certainly existed and
     * was certainly visible. Nobody else holds its key, so A is the only thing that can have taken it. That
     * is Rule 5, and it is the only way an operation with no trackable output is decided from state.
     */
    @Test
    fun `a coin moved out to an external asset resolves as success from the coin being gone`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val minter = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)
        chainReachesMortalityOf(minter, finality = FINALIZED)

        val offboard = givenUnwatchedOffboard(inputCoin = COIN_B)
        consumeCoinOnChain(COIN_B, finality = FINALIZED)
        runPass()

        assertEquals(FINALIZED_SUCCESS, statusOf(offboard))
    }

    /**
     * The same offboard, read while the block that consumed the coin is still only on the best chain.
     * C2 is gone there but still present at the finalized head.
     * A is a success, but not one the chain has settled.
     *
     * Rule 6 is the unfinalized twin of Rule 5: it keeps the entry out of the way without claiming more than
     * the chain has committed to.
     */
    @Test
    fun `the same spend seen only on the best chain is a success that is not yet final`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val minter = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = FINALIZED)
        chainReachesMortalityOf(minter, finality = FINALIZED)

        val offboard = givenUnwatchedOffboard(inputCoin = COIN_B)
        consumeCoinOnChain(COIN_B, finality = IN_BEST)
        runPass()

        assertEquals(PENDING_SUCCESS, statusOf(offboard))
    }

    /**
     * A payment never reaches a block and the app stays closed past its mortality.
     * On reopening, the node answers for the best head but not for the finalized one.
     * The payment must be failed and its coin returned, not held until the connection improves.
     *
     * Best-head evidence is a reason to wait inside the window and none outside it, where the transaction
     * can no longer execute. The window guard on Rules 3b and 4b is what drops the entry through to the
     * search, the only thing left that can decide it.
     */
    @Test
    fun `a payment that never landed is still failed when only the best head can be read`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        chainReachesMortalityOf(id, finality = FINALIZED)
        // The best head has to sit above the finalized one, or blinding one blinds both and the best head
        // has nothing to say either.
        advanceBlocks(1, finality = IN_BEST)
        makeCoinsUnreadableAtFinalizedHead()
        runPass()

        assertEquals(FAILURE, statusOf(id))
        assertNull("the coin is the user's to spend again", assetStateOf(COIN_A).consumerStatus?.takeIf { it.isLive })
    }

    private fun DurabilityHarness.blindPresence() = makeCoinsUnreadable(COIN_A, COIN_B, COIN_C, COIN_SEED)

    private fun statusEvents() = MutableSharedFlow<ExtrinsicStatus>(replay = 4, extraBufferCapacity = 8)

    private suspend fun DurabilityHarness.nonFailedClaimantsOf(coin: Int) = repository.getAllEntries().getOrThrow()
        .filter { it.status != DurableTxStatus.FAILURE }
        .count { entry -> entry.inputs.any { it.asset == OwnAsset.Coin(testKey(coin)) } }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val COIN_C = 3
private const val COIN_D = 4
private const val COIN_SEED = 9
