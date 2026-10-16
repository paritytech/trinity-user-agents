package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageRegistrationError
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.FINALIZED
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.TestActionFinality.IN_BEST
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import kotlinx.coroutines.flow.MutableSharedFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `coinage-durability-spec.md § Testing`, *Registration and submission* — all ten, each driven through the
 * real registrar and tracker rather than asserted against a model of them.
 */
class RegistrationScenariosTest {
    @Test
    fun `a crash right after the commit leaves the entry holding its inputs`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = register(COIN_A, COIN_B).getOrThrow()

        crash()

        assertEquals(PENDING, statusOf(id))
        assertEquals(PENDING, assetStateOf(COIN_A).consumerStatus)
    }

    @Test
    fun `a registration that rolls back leaves the watched set unchanged`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = register(COIN_A, COIN_B).getOrThrow()

        val second = register(COIN_A, COIN_C)

        assertTrue(second.exceptionOrNull() is CoinageRegistrationError.InputAlreadyClaimed)
        assertTrue(ownedEntries.isOwnedBySubmission(first))
        assertFalse(ownedEntries.isOwnedBySubmission(CoinageTransactionId(first.value + 1)))
    }

    @Test
    fun `an entry the tracker still owns receives no verdict from a pass`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        // Never completes, so the watcher keeps ownership for the whole test.
        submissionStatuses = { MutableSharedFlow() }
        val id = register(COIN_A, COIN_B).getOrThrow()

        // Evidence that would otherwise finalize it outright.
        mintCoinsOnChain(COIN_B, finality = IN_BEST)
        finalizeToBest()
        runPass()

        assertTrue(ownedEntries.isOwnedBySubmission(id))
        assertEquals(PENDING, statusOf(id))
    }

    @Test
    fun `a duplicate output address is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_D, finality = FINALIZED)
        register(COIN_A, COIN_B).getOrThrow()

        val second = register(COIN_D, COIN_B)

        assertTrue(second.exceptionOrNull() is CoinageRegistrationError.OutputNotFresh)
    }

    @Test
    fun `an entry with neither inputs nor outputs is rejected`() = scenario {
        val result = service.submitTransaction(
            extrinsic = extrinsicAnchoredAtFinalizedHead(),
            inputs = emptyList(),
            outputs = emptyList(),
            groupId = null,
        )

        assertTrue(result.exceptionOrNull() is CoinageRegistrationError.EmptyTransaction)
    }

    /**
     * The runtime enforces the era the extrinsic was signed with, so the window has to come from the
     * extrinsic and not from wherever the chain happens to be by the time registration runs. Anchoring to a
     * later head would search a range starting after the block the extrinsic could already have landed in.
     */
    @Test
    fun `the era anchors the checkpoint even when the chain has moved on`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        advanceBlocks(5, finality = FINALIZED)
        val anchor = chain.chain.finalizedHead.number

        // Built here, but the chain finalizes another block before the registration lands.
        val extrinsic = extrinsicAnchoredAtFinalizedHead()
        advanceBlocks(1, finality = FINALIZED)
        assertEquals(anchor + 1, chain.chain.finalizedHead.number)

        val id = service.submitTransaction(
            extrinsic = extrinsic,
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        ).getOrThrow()

        val entry = repository.getEntry(id).getOrThrow()!!
        assertEquals("the checkpoint is the era's birth block, not the newer finalized head", anchor, entry.checkpoint.blockNumber)
        assertEquals(chain.chain.canonicalAt(anchor)!!.hash, entry.checkpoint.blockHash)
        assertEquals(HARNESS_MORTAL_PERIOD.toLong(), entry.mortalityBlocks)
        assertEquals(anchor + HARNESS_MORTAL_PERIOD, entry.mortalityEnd)
    }

    /**
     * A period boundary crossed between building and registering, which every other scenario avoids by
     * anchoring at the head.
     *
     * An era is only (period, phase), so the birth block it names repeats every period and the era alone
     * cannot tell the occurrences apart. The extrinsic reports the block it was actually signed over, so the
     * original anchor is kept — and the window it implies has already closed, which is the truth.
     */
    @Test
    fun `an era whose period has elapsed keeps the block it was signed over`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val anchor = chain.chain.finalizedHead.number
        val extrinsic = extrinsicAnchoredAt(anchor)

        advanceBlocks(HARNESS_MORTAL_PERIOD + 2, finality = FINALIZED)

        val id = service.submitTransaction(
            extrinsic = extrinsic,
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        ).getOrThrow()

        val entry = repository.getEntry(id).getOrThrow()!!
        assertEquals("the signed anchor, not the current period's occurrence of the same phase", anchor, entry.checkpoint.blockNumber)
        assertEquals(chain.chain.canonicalAt(anchor)!!.hash, entry.checkpoint.blockHash)
        assertEquals("the window it implies has already closed", anchor + HARNESS_MORTAL_PERIOD, entry.mortalityEnd)
    }

    /**
     * Only extrinsics this app built are registrable. One signed elsewhere carries an era but cannot say
     * which block anchors it, and the era alone names a birth block that repeats every period, so there is
     * no window to record and the registration must be refused.
     */
    @Test
    fun `an extrinsic that cannot report its era anchor is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)

        val result = service.submitTransaction(
            extrinsic = externallySignedExtrinsic(
                hex = nextExtrinsicHex(),
                anchorBlock = chain.chain.finalizedHead.number,
                periodBlocks = HARNESS_MORTAL_PERIOD,
            ),
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        )

        assertTrue(result.exceptionOrNull() is CoinageRegistrationError.MissingEraAnchor)
        assertTrue(repository.getAllEntries().getOrThrow().isEmpty())
    }

    @Test
    fun `an extrinsic with no mortal era is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)

        val result = service.submitTransaction(
            extrinsic = immortalExtrinsic(),
            inputs = listOf(CoinageInput.Coin.Own(testKey(COIN_A))),
            outputs = listOf(OwnAsset.Coin(testKey(COIN_B))),
            groupId = null,
        )

        assertTrue(result.exceptionOrNull() is CoinageRegistrationError.NotMortal)
        assertTrue("nothing may be locked by an entry that could never resolve", repository.getAllEntries().getOrThrow().isEmpty())
    }

    @Test
    fun `an input already claimed by a finalized entry is rejected, not only by a live one`() = scenario {
        disableFallbackTxSearch()
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val first = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)

        mintCoinsOnChain(COIN_B, finality = IN_BEST)
        finalizeToBest()
        runPass()
        assertEquals(FINALIZED_SUCCESS, statusOf(first))

        val second = register(COIN_A, COIN_C)

        assertTrue(second.exceptionOrNull() is CoinageRegistrationError.InputAlreadyClaimed)
    }

    @Test
    fun `an input absent at the best head is still registered`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)

        // Gone above the finalized head: registration reads at F only, so this must not matter.
        consumeCoinOnChain(COIN_A, finality = IN_BEST)

        assertTrue(register(COIN_A, COIN_B).isSuccess)
    }

    @Test
    fun `a late event after release changes nothing and no resubmission follows`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val events = MutableSharedFlow<ExtrinsicStatus>(replay = 4, extraBufferCapacity = 8)
        submissionStatuses = { events }

        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        // Silence released the entry after the timeout, without the flow ever completing.
        assertFalse(ownedEntries.isOwnedBySubmission(id))
        val submissionsAtRelease = submissionCount

        events.tryEmit(ExtrinsicStatus.Finalized(chain.chain.bestHead.hash, "0xdead"))
        releaseSubmissions()

        assertEquals(PENDING, statusOf(id))
        assertEquals(submissionsAtRelease, submissionCount)
    }

    @Test
    fun `ownership is one-shot and is never taken back`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val id = givenUnwatchedEntry(inputCoin = COIN_A, outputCoin = COIN_B)
        assertFalse(ownedEntries.isOwnedBySubmission(id))

        ownedEntries.acquire(id, repository.getEntry(id).getOrThrow()!!.txHash)

        assertFalse(ownedEntries.isOwnedBySubmission(id))
    }

    /**
     * **Single handoff**: an asset leaves the device once, or the second memo carries the same private key
     * and two peers hold the same coin.
     *
     * The mark is asserted to survive a relaunch: a second mark would be uncommitted, so a rejection that
     * nonetheless wrote one would lose the committed mark the first handoff left.
     */
    @Test
    fun `a second handoff of an asset already handed off is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_A)))).getOrThrow().commit().getOrThrow()

        val second = service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_A))))

        assertEquals(
            CoinageRegistrationError.HandoffOfHandedOffAsset(OwnAsset.Coin(testKey(COIN_A))),
            second.exceptionOrNull(),
        )
        assertTrue(assetStateOf(COIN_A).handedOff)
        relaunch()
        assertTrue("the first mark is the one that stays in place", assetStateOf(COIN_A).handedOff)
    }

    @Test
    fun `a handoff whose mark was released can be marked again`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)
        val handoff = service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_A)))).getOrThrow()

        handoff.release().getOrThrow()

        assertFalse("the payment never went through, so nothing left the device", assetStateOf(COIN_A).handedOff)
        assertTrue(service.preCommitHandoff(listOf(OwnAsset.Coin(testKey(COIN_A)))).isSuccess)
        assertTrue(assetStateOf(COIN_A).handedOff)
    }

    @Test
    fun `a batch registers every transaction under its group and takes ownership of each`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_D, finality = FINALIZED)

        val ids = registerGroup(COIN_A to COIN_B, COIN_D to COIN_C, groupId = GROUP).getOrThrow()

        assertEquals(2, ids.size)
        ids.forEach { assertTrue("the tracker owns every entry of the group", ownedEntries.isOwnedBySubmission(it)) }
        ids.forEach { assertEquals(GROUP, repository.getEntry(it).getOrThrow()!!.groupId) }
    }

    /**
     * The **Fresh outputs** invariant inside one batch. The rows do not exist yet, so `filterMinted` cannot
     * see the collision and only an in-batch check catches it — the input side has always had one.
     *
     * Without it both entries register and claim the same output address, and `minterByKey` then keeps one of
     * them arbitrarily, so one entry is decided against the other's evidence.
     */
    @Test
    fun `a batch minting the same output address twice is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_D, finality = FINALIZED)

        val result = registerGroup(COIN_A to COIN_B, COIN_D to COIN_B, groupId = GROUP)

        assertTrue(result.exceptionOrNull() is CoinageRegistrationError.OutputNotFresh)
    }

    @Test
    fun `a batch claiming the same input twice is rejected`() = scenario {
        mintCoinsOnChain(COIN_A, finality = FINALIZED)

        val result = registerGroup(COIN_A to COIN_B, COIN_A to COIN_C, groupId = GROUP)

        assertTrue(result.exceptionOrNull() is CoinageRegistrationError.InputAlreadyClaimed)
    }

    /** Half a group in the ledger is indistinguishable from a group whose other half failed on chain. */
    @Test
    fun `a batch whose second transaction is invalid registers neither`() = scenario {
        mintCoinsOnChain(COIN_A, COIN_D, finality = FINALIZED)
        val existing = givenUnwatchedEntry(inputCoin = COIN_D, outputCoin = COIN_C)

        // The second collides with an entry that already exists, so the batch cannot be recorded at all.
        val result = registerGroup(COIN_A to COIN_B, COIN_D to COIN_E, groupId = GROUP)

        assertTrue(result.isFailure)
        assertNull("the first must not survive its group", assetStateOf(COIN_B).minterStatus)
        assertNull("nor may it hold the input it claimed", assetStateOf(COIN_A).consumerStatus)
        assertEquals(PENDING, statusOf(existing))
    }
}

private val GROUP = CoinageOperationGroupId("group-under-test")

private const val COIN_A = 1
private const val COIN_B = 2
private const val COIN_C = 3
private const val COIN_D = 4
private const val COIN_E = 5
