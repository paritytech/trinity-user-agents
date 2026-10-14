package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ExtrinsicOutcome
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Everything that turns on the best chain being rewritten under an entry.
 *
 * A reorg is the one event that can retract evidence the subsystem has already acted on, so every verdict
 * resting on a block above the finalized head has to survive losing that block. `CoinageRulesTest` pins the
 * canonicality clauses of Rule 0 against hand-built evidence; these drive the same paths through a real
 * block tree, where a retracted block also takes its state and its dispatch outcome with it.
 */
class ReorgScenariosTest {
    @Test
    fun `Rule 0 clause 1 clears the record and demotes when the recorded block is gone`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)
        assertEquals(PENDING_SUCCESS, statusOf(id))
        assertNotNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)

        reorgLastBlocks(1)
        advanceBlocks(1, finality = IN_BEST)
        runPass()

        assertEquals(PENDING, statusOf(id))
        assertNull(
            "the outputs must lose optimistic selectability with the record",
            repository.getEntry(id).getOrThrow()!!.successDetectedAt,
        )
    }

    @Test
    fun `an output reorged out becomes nonexistent and its consumer fails on its own window`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val minter = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        mintCoinsOnChain(COIN_B, finality = IN_BEST)
        runPass()
        assertEquals(PENDING_SUCCESS, statusOf(minter))

        reorgLastBlocks(1)
        advanceBlocks(1, finality = IN_BEST)
        runPass()
        assertEquals(PENDING, statusOf(minter))

        chainReachesMortalityOf(minter, finality = FINALIZED)
        runPass()
        assertEquals(FAILURE, statusOf(minter))
        assertEquals(FAILURE, assetStateOf(COIN_B).minterStatus)
    }

    /**
     * Tx A is seen in a best-chain block and recorded as a pending success, which keeps its output spendable.
     * The node then goes unreachable.
     * The next pass cannot tell whether that block is still on the chain.
     * A must keep its pending success, and its output must stay spendable.
     *
     * A failed read is not a retraction. Treating it as one would demote the entry — and withdraw a coin the
     * user could otherwise spend — every time the connection blips.
     */
    @Test
    fun `an unreachable node does not withdraw a success that was already detected`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)
        val recorded = repository.getEntry(id).getOrThrow()!!.successDetectedAt!!

        // The chain finalizes past the recorded block before the read fails. That is what makes the three
        // readings of it disagree: a block read as canonical would finalize the entry outright, and one read
        // as gone would re-record a newer head. Only "could not tell" leaves both of these untouched.
        advanceBlocks(2, finality = FINALIZED)
        makeBlocksUnreadable(recorded.blockNumber)
        runPass()

        assertEquals(PENDING_SUCCESS, statusOf(id))
        assertEquals(recorded, repository.getEntry(id).getOrThrow()!!.successDetectedAt)
    }

    /**
     * Tx A is seen in a best-chain block and recorded there as a pending success.
     * A reorg then leaves the chain shorter than the block that record names.
     * The record must go: the chain does not have that block, so nothing is holding A's outputs up.
     *
     * A height the chain has not reached is the chain answering, not failing to answer. Read as "cannot
     * tell", the record survives every pass and A's outputs stay spendable on a block that is gone — until
     * the chain happens to grow back to that height. Found by the fuzzer, shrunk to six actions.
     */
    @Test
    fun `a reorg that shortens the chain past a recorded block clears the record`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryDecided(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)
        val recorded = repository.getEntry(id).getOrThrow()!!.successDetectedAt!!

        // Dropped, not replaced, so the best head now sits below the height the record names.
        reorgLastBlocks(chain.chain.reorgDepths.last)
        assertTrue(chain.chain.bestHead.number < recorded.blockNumber)
        runPass()

        assertNull(repository.getEntry(id).getOrThrow()!!.successDetectedAt)
        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `an outcome is read from the same block the extrinsic was found in`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        val txHash = repository.getEntry(id).getOrThrow()!!.txHash

        // The same transaction applied in two blocks with opposite outcomes: only the canonical one counts,
        // so a block reordered out of the chain must not lend its outcome to the other.
        val orphaned = includeInBlock(txHash, ExtrinsicOutcome.FAILURE, finality = IN_BEST)
        reorgLastBlocks(1)
        includeInBlock(txHash, ExtrinsicOutcome.SUCCESS, finality = IN_BEST)
        // Blinded so no presence rule can decide first and the entry has to reach the search.
        makeCoinsUnreadable(COIN_A, COIN_B)
        chainReachesMortalityOf(id, finality = FINALIZED)
        runPass()

        assertEquals(ExtrinsicOutcome.FAILURE, chain.chain.blockAt(orphaned.hash)!!.state.outcomes[txHash])
        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    /**
     * Tx A consumes C1 and mints C2, and is seen in a best-chain block.
     * Our own transaction B is then registered to spend C2.
     * The block carrying A is reorged out, taking A and C2 with it.
     * Both must end up failed.
     *
     * They fail on different rules. Rule 3 is shut off for A — C2 is absent, but B still claims it — and
     * Rule 4 decides A instead: the retraction returns C1 to the finalized head, where it reads as available
     * to spend again. B has no such input, so Rule 3 is what fails it.
     */
    @Test
    fun `a retracted mint fails both the entry that minted the coin and the entry spending it`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        // Executed in a best-chain block, so the reorg below retracts the minter's own transaction and the
        // coin it minted together — not just the coin.
        val minter = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)
        runPass()
        assertEquals(PENDING_SUCCESS, statusOf(minter))

        val consumer = givenUnwatchedEntry(inputCoin = COIN_B, outputCoin = COIN_C)

        reorgLastBlocks(1)
        advanceBlocks(1, finality = IN_BEST)
        runPass()
        assertEquals(PENDING, statusOf(minter))
        assertEquals(PENDING, statusOf(consumer))

        chainReachesMortalityOf(consumer, finality = FINALIZED)
        runPass()

        assertEquals(FAILURE, statusOf(minter))
        assertEquals(FAILURE, statusOf(consumer))
    }

    /**
     * Tx A executes in a block, and a pass records its success at the best head one block higher.
     * That head is reorged away and replaced, while A's own block goes on to finalize.
     * The chain grows once more, so the best head is above the finalized one again.
     * A must finalize: the block it executed in can no longer be rewritten.
     */
    @Test
    fun `a head reorged between passes does not hold up a transaction finalized below it`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenEntryExecutedOnChain(inputCoin = COIN_A, outputCoin = COIN_B, finality = IN_BEST)
        val txBlock = chain.chain.bestHead.number

        advanceBlocks(1, finality = IN_BEST)
        runPass()
        assertEquals(PENDING_SUCCESS, statusOf(id))

        // Only the head above the transaction's block is rewritten, so that block finalizes with the rest.
        reorgLastBlocks(1)
        advanceBlocks(1, finality = FINALIZED)
        advanceBlocks(1, finality = IN_BEST)
        runPass()

        assertTrue("the transaction's block is final", txBlock <= chain.chain.finalizedHead.number)
        assertEquals(FINALIZED_SUCCESS, statusOf(id))
    }

    /**
     * A claim of a peer's coin lands in a best-chain block, and a payment of ours spending the claimed coin is
     * registered on the strength of it.
     * A fork takes the claim's block away, so the peer's coin is back and ours never existed.
     * The claim is built again into the same coin, and once it lands the payment spending that coin completes.
     *
     * A claim retried into a fresh coin would leave the payment waiting forever on one that will never exist.
     */
    @Test
    fun `a claim a fork invalidated is built again into the same coin`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(PEER_COIN, finality = FINALIZED)
        val policy = givenSubmissionPolicy(PolicyBehaviour.BUILD)

        val claim = service.submitTransactions(
            transactions = listOf(
                CoinageTransactionRequest(
                    extrinsic = extrinsicAnchoredAtFinalizedHead(),
                    inputs = listOf(CoinageInput.Coin.Received(coinKeyOf(PEER_COIN))),
                    outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
                    policy = scriptedPolicy(),
                )
            ),
            groupId = CoinageOperationGroupId("claim"),
        ).getOrThrow().single()
        val spender = registerRetriable(inputCoin = COIN_B, COIN_C)
        releaseSubmissions()

        consumeCoinOnChain(PEER_COIN, finality = IN_BEST)
        mintCoinsOnChain(COIN_B, finality = IN_BEST)
        runPass()
        assertEquals(PENDING_SUCCESS, statusOf(claim))

        reorgLastBlocks(2)
        advanceBlocks(1, finality = IN_BEST)
        runPass()
        assertEquals(PENDING, statusOf(claim))

        chainReachesMortalityOf(claim, finality = FINALIZED)
        runPass()
        releaseSubmissions()

        assertTrue("the claim was built again", policy.prepared.flatten().contains(claim))
        assertEquals(PENDING, statusOf(claim))
        assertEquals(
            "the rebuilt claim mints the coin the payment spends",
            listOf(OwnAsset.Coin(testKey(COIN_B))),
            ledger.coinage.assetsOf(listOf(claim)).getOrThrow().getValue(claim).outputs.map { it.asset },
        )

        consumeCoinOnChain(PEER_COIN, finality = FINALIZED)
        mintCoinsOnChain(COIN_B, finality = FINALIZED)
        runPass()
        assertEquals(FINALIZED_SUCCESS, statusOf(claim))

        consumeCoinOnChain(COIN_B, finality = FINALIZED)
        mintCoinsOnChain(COIN_C, finality = FINALIZED)
        runPass()
        assertEquals(FINALIZED_SUCCESS, statusOf(spender))
    }
}

private const val COIN_A = 1
private const val COIN_B = 2
private const val COIN_C = 3
private const val PEER_COIN = 50
