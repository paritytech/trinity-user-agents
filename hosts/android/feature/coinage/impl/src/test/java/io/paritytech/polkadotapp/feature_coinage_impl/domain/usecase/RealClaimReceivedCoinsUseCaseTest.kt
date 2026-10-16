package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetValueUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness.RecordingConnectionRefCounter
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FAILURE
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUBMISSION
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING_SUCCESS
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Claiming coins a peer handed us, and retrying the claims that did not take.
 *
 * The claim is not a one-shot: a coin visible on chain and not yet claimed is money the peer has already
 * parted with, so it is worth retrying for as long as the caller's window allows. What the tests here pin is
 * when a retry may happen at all — because the two ways to get it wrong are opposite and both expensive.
 * Claiming a coin twice mints a second transaction against a key that is already spent; giving up on one
 * strands funds that nothing else in the app will collect.
 */
@OptIn(ExperimentalTime::class)
class RealClaimReceivedCoinsUseCaseTest {
    @Before
    fun mockDerivation() = mockkStatic(DERIVATION_FILE)

    @After
    fun unmockDerivation() = unmockkStatic(DERIVATION_FILE)

    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinRepository: CoinRepository = mockk()
    private val transactionService: CoinageTransactionService = mockk()
    private val assetValueUseCase: CoinageAssetValueUseCase = mockk()
    private val submissionUseCase: CoinageTransferSubmissionUseCase = mockk()
    private val timeProvider: TimeProvider = mockk()
    private val connections = RecordingConnectionRefCounter()

    private val useCase = RealClaimReceivedCoinsUseCase(
        chainAssetProvider = chainAssetProvider,
        coinRepository = coinRepository,
        transactionService = transactionService,
        assetValueUseCase = assetValueUseCase,
        submissionUseCase = submissionUseCase,
        timeProvider = timeProvider,
        chainConnectionRefCounter = connections,
    )

    private val groupId = CoinageOperationGroupId("group")

    @Before
    fun openTheWindow() {
        every { chainAssetProvider.chainId() } returns "test-chain"
        every { timeProvider.now() } returns WINDOW_OPEN
        coEvery { submissionUseCase(any(), any(), any(), any()) } returns Result.success(Unit)
        coEvery { assetValueUseCase.valueOf(any()) } answers {
            // One unit per minted output, so what a claim is said to be worth stays visible in assertions.
            Result.success(BigInteger.valueOf(firstArg<List<OwnAsset>>().size.toLong()).intoBalance())
        }
    }

    // ---- when a claim may be submitted ----

    /**
     * A first attempt: the group is empty and both coins the peer named are on chain.
     * They are claimed once, under the caller's group.
     */
    @Test
    fun `coins found on chain are claimed under the caller's group`() = runTest {
        val first = key(1)
        val second = key(2)
        givenChainSees(listOf(first.accountId, second.accountId))
        givenGroupReports(noEntries())

        reportsOf(first, second)

        assertClaimedOnce(first.keypair, second.keypair)
    }

    /**
     * A claim's attempt was proven unable to land, and the engine is building it again into the same coin.
     * The coin is still on chain, but that is the rebuild's to claim — a second claim here would be refused
     * at best and mint a coin nobody is waiting for at worst. The claim stays open until the rebuild settles.
     */
    @Test
    fun `a failed claim with a retry policy is left to the engine`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(PENDING_SUBMISSION, claiming = coin.accountId)))

        assertDoesNotComplete(coin)

        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * The claim for this coin failed for good: its policy already rebuilt it for as long as it could, into the
     * coin it recorded. A new claim here would mint into a different coin, which a payment made out of the
     * recorded one would never see — so the claim ends instead, however much of the coin is still on chain.
     */
    @Test
    fun `a failed claim is never claimed again into a new coin`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(FAILURE, claiming = coin.accountId)))

        val reported = reportsOfCompleted(coin)

        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
        assertEquals(CoinageTransferDetection.NotClaimed, reported.last())
    }

    /**
     * A claim of ours already finalized against this coin, so the coin is ours and the chain read that still
     * shows it is stale. Claiming it again would spend a key that is already spent.
     */
    @Test
    fun `a coin a finalized claim already took is never claimed again`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, claiming = coin.accountId)))

        reportsOf(coin)

        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * An earlier attempt failed but a later one is still in flight. The coin is on chain because that
     * attempt has not executed yet — not because it needs claiming again.
     */
    @Test
    fun `a failed claim is not submitted again while another attempt is still live`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(
            listOf(
                entry(FAILURE, claiming = coin.accountId),
                entry(PENDING, claiming = coin.accountId),
            )
        )

        reportsOf(coin)

        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    /** Nothing to claim: submitting against a coin the chain does not hold only gets it refused. */
    @Test
    fun `a coin that is not on chain yet is not claimed`() = runTest {
        val coin = key(1)
        givenChainSees(emptyList())
        givenGroupReports(noEntries())

        reportsOf(coin)

        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * The sender's split had not landed when the message arrived, so the first look found nothing. The coin
     * appears a few blocks later and is claimed then, without anything having to poll for it.
     */
    @Test
    fun `a coin that appears later is claimed when it does`() = runTest {
        val coin = key(1)
        givenChainSees(emptyList(), listOf(coin.accountId))
        givenGroupReports(noEntries(), noEntries())

        reportsOf(coin)

        assertClaimedOnce(coin.keypair)
    }

    /**
     * One coin is visible and another is still landing. Claiming waits for the pair rather than taking the
     * one it can see, and then claims both together.
     *
     * Submitting while the sender's split is still settling is the leading explanation for a claim being
     * refused outright, which is how 0.88 of a 1.00 payment went unclaimed.
     */
    @Test
    fun `claiming holds out for the whole set before submitting`() = runTest {
        val first = key(1)
        val second = key(2)
        givenChainSees(listOf(first.accountId), listOf(first.accountId, second.accountId))
        givenGroupReports(noEntries())

        reportsOf(first, second)

        assertClaimedOnce(first.keypair, second.keypair)
    }

    /** Only what is actually on chain is claimed; the coin still missing is left for a later look. */
    @Test
    fun `a partial detection claims only the coins that arrived`() = runTest {
        val arrived = key(1)
        val missing = key(2)
        givenChainSees(listOf(arrived.accountId))
        givenGroupReports(noEntries())

        reportsOf(arrived, missing)

        assertClaimedOnce(arrived.keypair)
    }

    /**
     * A claim is in flight and the chain is looked at again — the coin is still there, because the claim
     * that will take it has not executed yet.
     *
     * Waiting for the ledger to settle before claiming again is the whole protection against claiming a
     * coin twice; a fresh look at the chain is not a reason to submit while an attempt is outstanding.
     */
    @Test
    fun `a coin is not claimed again while the claim for it is still in flight`() = runTest {
        val coin = key(1)
        val submitted = CompletableDeferred<Unit>()

        givenLedgerRegistersOnSubmit(PENDING, claiming = coin.accountId, signal = submitted)
        givenChainSeesAgainAfter(submitted)

        reportsOf(coin)

        coVerify(exactly = 1) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * The ledger refused the claim, so nothing was registered and the coin is untouched. The next look at
     * the chain has to try it again — a refusal that quietly counted as an attempt would strand the coin
     * exactly as surely as never trying at all.
     */
    @Test
    fun `a submission the ledger refuses is tried again on the next look`() = runTest {
        val coin = key(1)
        val refused = CompletableDeferred<Unit>()

        givenSubmissionSignals(refused, Result.failure(IllegalStateException("refused")))
        givenChainSeesAgainAfter(refused)
        givenGroupReports(noEntries())

        reportsOf(coin)

        coVerify(exactly = 2) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * One coin is seen, then a fork takes it away and a different one appears. Only the coin the chain
     * currently holds is claimed.
     *
     * Claiming against the widest view ever seen would submit against a coin that has since been reorged
     * out — which is exactly the refusal this whole loop exists to recover from.
     */
    @Test
    fun `a coin a fork took away is not claimed on the strength of an older look`() = runTest {
        val gone = key(1)
        val arrived = key(2)
        givenChainSees(listOf(gone.accountId), listOf(arrived.accountId))
        givenGroupReports(noEntries())

        reportsOf(gone, arrived)

        assertClaimedOnce(arrived.keypair)
    }

    /**
     * A read of the chain fails between two looks. It is not a look at all, so it neither claims anything
     * nor erases what the previous look established — absence and ignorance are not the same thing.
     */
    @Test
    fun `a failed read does not erase what the chain last showed`() = runTest {
        val seen = key(1)
        val missing = key(2)
        givenChainReads(
            listOf(
                Result.success(listOf(seen.accountId)),
                Result.failure(IllegalStateException("no connection")),
            )
        )
        givenGroupReports(noEntries())

        reportsOf(seen, missing)

        assertClaimedOnce(seen.keypair)
    }

    /**
     * A failed read ends a real storage subscription instead of being one look among others. A claim that
     * stopped watching there would never see the coin arrive, and with no deadline it would never end either.
     */
    @Test
    fun `a claim watches the chain again after a failed read ends the subscription`() = runTest {
        val coin = key(1)
        givenChainFailsOnceThenSees(listOf(coin.accountId))
        givenGroupReports(noEntries())

        reportsOf(coin)

        assertClaimedOnce(coin.keypair)
    }

    /**
     * Nothing else keeps the asset chain connected while the app is in the background, and a claim waiting
     * on a paused socket hears nothing. The claim holds the connection itself, and only while it runs.
     */
    @Test
    fun `a claim keeps the chain connected while it watches and lets go when it ends`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, claiming = coin.accountId)))

        reportsOfCompleted(coin)

        assertEquals(1, connections.peak)
        assertEquals(0, connections.held)
    }

    // ---- when the retrying stops ----

    /**
     * Every coin has a finalized claim, so there is nothing left that retrying could win. Inclusion would
     * not have been enough: a fork can take a block away, and a payment closed on an inclusion that is later
     * retracted is a payment nothing will ever try again.
     */
    @Test
    fun `claiming ends once every coin has a finalized claim`() = runTest {
        val first = key(1)
        val second = key(2)
        givenChainSees(listOf(first.accountId, second.accountId))
        givenGroupReports(
            listOf(
                entry(FINALIZED_SUCCESS, claiming = first.accountId),
                entry(FINALIZED_SUCCESS, claiming = second.accountId),
            )
        )

        val reported = reportsOfCompleted(first, second)

        assertEquals(CoinageTransferDetection.Claimed(TWO_ASSETS, finalized = true), reported.last())
    }

    /** A claim in a block is reported as received, but it is not yet a reason to stop watching it. */
    @Test
    fun `a claim only included in a block does not end the claiming`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(PENDING_SUCCESS, claiming = coin.accountId)))

        assertDoesNotComplete(coin)
    }

    /**
     * The window closed and the coins never turned up. Nothing more will be tried and the caller is told so,
     * because waiting on a coin that has not appeared is the one thing a timer can settle: nothing else can
     * tell a sender who was slow from one who never sent.
     */
    @Test
    fun `claiming ends when the window closes on coins that never arrived`() = runTest {
        val coin = key(1)
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenChainSees(emptyList())
        givenGroupReports(noEntries())

        val reported = reportsOfCompleted(coin)

        assertEquals(CoinageTransferDetection.NotClaimed, reported.last())
        coVerify(exactly = 0) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * The window closed without the chain ever answering — a socket that stayed paused in the background
     * looks exactly like this. Silence is not evidence that the coins never arrived, so it settles nothing.
     */
    @Test
    fun `the window closing does not end a claim the chain never answered`() = runTest {
        val coin = key(1)
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenChainSees()
        givenGroupReports(noEntries())

        assertDoesNotComplete(coin)
    }

    /**
     * A payment received long ago whose coin only now shows on chain. It is still the peer's money waiting to
     * be collected, and the window has nothing to say about it.
     *
     * Giving up here would abandon funds permanently: nothing else in the app collects a coin handed over in
     * a chat, and the sender cannot take it back.
     */
    @Test
    fun `a coin still on chain is claimed however long ago the payment arrived`() = runTest {
        val coin = key(1)
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(noEntries())

        reportsOf(coin)

        assertClaimedOnce(coin.keypair)
    }

    /** A claim the ledger refused is registered again for as long as the chain keeps showing the coin, window or no window. */
    @Test
    fun `claiming carries on past the window while the coin is still there`() = runTest {
        val coin = key(1)
        val refused = CompletableDeferred<Unit>()
        every { timeProvider.now() } returns WINDOW_CLOSED

        givenSubmissionSignals(refused, Result.failure(IllegalStateException("refused")))
        givenChainSeesAgainAfter(refused)
        givenGroupReports(noEntries())

        reportsOf(coin)

        coVerify(exactly = 2) { submissionUseCase(any(), any(), any(), any()) }
    }

    /**
     * A message first seen long after it was sent — the app was closed, or the device was off.
     *
     * The window bounds retrying, not trying. Refusing to attempt at all would turn a late look at a
     * perfectly claimable coin into money stranded for good.
     */
    @Test
    fun `a first attempt is made even when the window has already closed`() = runTest {
        val coin = key(1)
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(noEntries())

        reportsOfCompleted(coin)

        assertClaimedOnce(coin.keypair)
    }

    /** A coin that never appeared keeps its chance for as long as the window is open. */
    @Test
    fun `a coin that has not appeared keeps the claim open`() = runTest {
        val coin = key(1)
        givenChainSees(emptyList())
        givenGroupReports(noEntries())

        assertDoesNotComplete(coin)
    }

    // ---- what the caller is told ----

    @Test
    fun `a claim reports itself as detecting before it knows anything`() = runTest {
        val coin = key(1)
        givenChainSees(emptyList())
        givenGroupReports(noEntries())

        val reported = reportsOf(coin)

        assertEquals(CoinageTransferDetection.Detecting, reported.first())
    }

    /**
     * A payment of several coins arriving one at a time, with nothing wrong.
     *
     * It reports as still claiming, and deliberately says nothing about how much has landed yet: on the
     * happy path claims land one by one, so reporting the running total would walk the user through
     * "claimed 0.08, waiting" then "claimed 0.12, waiting" for a payment that is simply in progress.
     */
    @Test
    fun `a payment whose claims are merely still in flight reports as claiming`() = runTest {
        val first = key(1)
        val second = key(2)
        givenChainSees(listOf(first.accountId, second.accountId))
        givenGroupReports(
            listOf(
                entry(PENDING_SUCCESS, claiming = first.accountId),
                entry(PENDING, claiming = second.accountId),
            )
        )

        val reported = reportsOf(first, second)

        assertEquals(CoinageTransferDetection.Claiming, reported.last())
    }

    /**
     * The same shape, except the coin that has not arrived had its claim refused, so it is waiting on a
     * retry rather than simply on a block.
     *
     * Now the amount is worth reporting: part of the payment is settled and the rest genuinely hangs in the
     * balance. Calling this a completed transfer is what made a 1.00 payment show as 0.12 on a device.
     */
    @Test
    fun `a payment held up by a failed claim reports what has landed`() = runTest {
        val arrived = key(1)
        val refused = key(2)
        givenChainSees(listOf(arrived.accountId, refused.accountId))
        givenGroupReports(
            listOf(
                entry(PENDING_SUCCESS, claiming = arrived.accountId),
                entry(FAILURE, claiming = refused.accountId),
            )
        )

        val reported = reportsOf(arrived, refused)

        assertEquals(CoinageTransferDetection.ClaimingRest(ONE_ASSET), reported.last())
    }

    /**
     * A fork takes away the block a claim was seen in, so the ledger lowers it back to pending. The report
     * has to follow the ledger down again rather than leave the user told they were paid.
     */
    @Test
    fun `a claim retracted by a fork stops being reported as transferred`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(
            listOf(entry(PENDING_SUCCESS, claiming = coin.accountId)),
            listOf(entry(PENDING, claiming = coin.accountId)),
        )

        val reported = reportsOf(coin)

        assertEquals(
            listOf(
                CoinageTransferDetection.Detecting,
                CoinageTransferDetection.Claimed(ONE_ASSET, finalized = false),
                CoinageTransferDetection.Claiming,
            ),
            reported,
        )
    }

    /**
     * A retry left the group holding the dead outputs of the attempt that failed. They were never minted on
     * chain, so counting them would report a payment as worth more than it is.
     */
    @Test
    fun `the reported amount does not count a claim that failed`() = runTest {
        val coin = key(1)
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(
            listOf(
                entry(FAILURE, claiming = coin.accountId, outputs = 3),
                entry(PENDING_SUCCESS, claiming = coin.accountId),
            )
        )

        val reported = reportsOf(coin)

        assertEquals(CoinageTransferDetection.Claimed(ONE_ASSET, finalized = false), reported.last())
    }

    /**
     * A peer hands over two coins and only one claim executes before the window closes.
     * The coins that claim minted are ours regardless of what happened to its sibling.
     */
    @Test
    fun `one claim succeeding makes the payment received even though its sibling failed`() = runTest {
        val succeeded = key(1)
        val failed = key(2)
        every { timeProvider.now() } returns WINDOW_CLOSED
        givenChainSees(emptyList())
        givenGroupReports(
            listOf(
                entry(FINALIZED_SUCCESS, claiming = succeeded.accountId),
                entry(FAILURE, claiming = failed.accountId, outputs = 3),
            )
        )

        val reported = reportsOfCompleted(succeeded, failed)

        assertEquals(CoinageTransferDetection.ClaimedPartially(ONE_ASSET), reported.last())
    }

    /** A submission the ledger refuses is not the end of it — the coin is still there to be claimed. */
    @Test
    fun `a submission the ledger refuses leaves the claim open`() = runTest {
        val coin = key(1)
        coEvery { submissionUseCase(any(), any(), any(), any()) } returns Result.failure(IllegalStateException("refused"))
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(noEntries())

        assertDoesNotComplete(coin)
    }

    /**
     * The claimed coins cannot be valued — that needs chain metadata, which may not be to hand.
     *
     * The claim still reports where it got to, for nothing. Saying nothing would leave a payment that
     * actually arrived looking as though it never did.
     */
    @Test
    fun `a claim whose coins cannot be valued still reports that they arrived`() = runTest {
        val coin = key(1)
        coEvery { assetValueUseCase.valueOf(any()) } returns Result.failure(IllegalStateException("no metadata"))
        givenChainSees(listOf(coin.accountId))
        givenGroupReports(listOf(entry(FINALIZED_SUCCESS, claiming = coin.accountId)))

        val reported = reportsOfCompleted(coin)

        assertEquals(CoinageTransferDetection.Claimed(Balance.ZERO, finalized = true), reported.last())
    }

    // ---- harness ----

    /**
     * Exactly one submission was made, and it carried exactly these keys.
     *
     * Both halves matter: `exactly = 1` on its own counts only the calls that match, so a second submission
     * of a different shape — the very thing double-claiming looks like — would go unnoticed.
     */
    private fun assertClaimedOnce(vararg keypairs: Keypair) {
        // Read outside the verification block, which would otherwise record these reads as calls to verify.
        val keys = keypairs.map { it.publicKey.toDataByteArray() }

        coVerify(exactly = 1) { submissionUseCase(any(), any(), any(), any()) }
        coVerify(exactly = 1) { submissionUseCase(keys, any(), groupId, RETRY_UNTIL) }
    }

    private fun claimOf(vararg coins: PeerCoin): Flow<CoinageTransferDetection> =
        useCase.claim(coins.map { it.privateKey }, groupId, RETRY_UNTIL)

    /**
     * Everything the claim says before it either finishes or runs out of anything to react to.
     *
     * A claim is driven by two subscriptions, so how many times it re-evaluates is the combining's business
     * and not something a test should pin. What is worth pinning is what it ends up reporting.
     */
    private suspend fun reportsOf(vararg coins: PeerCoin): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        withTimeoutOrNull(IDLE) { claimOf(*coins).collect { reported += it } }

        return reported
    }

    /** Drains the claim and insists it actually finished — that nothing more would ever be attempted. */
    private suspend fun reportsOfCompleted(vararg coins: PeerCoin): List<CoinageTransferDetection> {
        val reported = mutableListOf<CoinageTransferDetection>()

        val completed = withTimeoutOrNull(IDLE) {
            claimOf(*coins).collect { reported += it }
            true
        }

        assertTrue("the claim was left open when nothing more would be attempted", completed == true)

        return reported
    }

    private suspend fun assertDoesNotComplete(vararg coins: PeerCoin) {
        var completed = false

        withTimeoutOrNull(IDLE) {
            claimOf(*coins).collect { }
            completed = true
        }

        assertFalse("the claim was closed while there was still something to claim", completed)
    }

    private class PeerCoin(val privateKey: CoinPrivateKey, val keypair: Keypair) {
        val accountId: AccountId = keypair.publicKey.toDataByteArray()
    }

    /** Derivation is a top-level extension over the SDK's sr25519 factory; only the seam matters here. */
    private fun key(seed: Int): PeerCoin {
        val privateKey: CoinPrivateKey = byteArrayOf(seed.toByte()).toDataByteArray()
        val keypair: Sr25519Keypair = mockk()

        every { keypair.publicKey } returns privateKey.value
        every { privateKey.deriveKeypair() } returns keypair

        return PeerCoin(privateKey, keypair)
    }

    /**
     * What the chain holds, one emission per look. A storage subscription stays open, so the flow does not
     * end after the last emission — which is what lets a claim stay open waiting for a coin to appear.
     */

    /** Raw chain reads, so a failed one can be placed between two good looks. */
    private fun givenChainReads(reads: List<Result<List<AccountId>>>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()

            flow {
                reads.forEach { read ->
                    emit(
                        read.map { present ->
                            requested.associateWith { accountId ->
                                OnChainCoinInfo(instanceId = 0, value = 3, age = 0).takeIf { accountId in present }
                            }
                        }
                    )
                }
                awaitCancellation()
            }
        }
    }

    private fun givenChainSees(vararg looks: List<AccountId>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()

            flow {
                looks.forEach { present ->
                    emit(Result.success(requested.associateWith { accountId ->
                        OnChainCoinInfo(instanceId = 0, value = 3, age = 0).takeIf { accountId in present }
                    }))
                }
                awaitCancellation()
            }
        }
    }

    /** The first subscription ends on a failed read, as the repository's does; the next one sees [present]. */
    private fun givenChainFailsOnceThenSees(present: List<AccountId>) {
        var subscriptions = 0

        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()

            if (subscriptions++ == 0) {
                flowOf(Result.failure(IllegalStateException("decode failed")))
            } else {
                flow {
                    emit(Result.success(requested.associateWith { accountId ->
                        OnChainCoinInfo(instanceId = 0, value = 3, age = 0).takeIf { accountId in present }
                    }))
                    awaitCancellation()
                }
            }
        }
    }

    /** A ledger that registers what it is handed, so a submission changes what the next pass reads. */
    private fun givenLedgerRegistersOnSubmit(
        status: DurableTxStatus,
        claiming: AccountId,
        signal: CompletableDeferred<Unit>,
    ) {
        val ledger = MutableStateFlow(noEntries())

        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns ledger
        coEvery { submissionUseCase(any(), any(), any(), any()) } answers {
            ledger.value = listOf(entry(status, claiming = claiming))
            signal.complete(Unit)

            Result.success(Unit)
        }
    }

    private fun givenSubmissionSignals(signal: CompletableDeferred<Unit>, outcome: Result<Unit>) {
        coEvery { submissionUseCase(any(), any(), any(), any()) } answers {
            signal.complete(Unit)
            outcome
        }
    }

    /** A second look at an unchanged chain, taken only once a claim has been attempted. */
    private fun givenChainSeesAgainAfter(attempted: CompletableDeferred<Unit>) {
        coEvery { coinRepository.subscribeCoinsInfoFor(any(), any()) } answers {
            val requested = secondArg<List<AccountId>>()
            val present = Result.success(requested.associateWith { OnChainCoinInfo(instanceId = 0, value = 3, age = 0) })

            flow {
                emit(present)
                attempted.await()
                emit(present)
                awaitCancellation()
            }
        }
    }

    private fun givenGroupReports(vararg emissions: List<CoinageTransactionState>) {
        every { transactionService.subscribeOperationGroupStatuses(groupId) } returns flow {
            emissions.forEach { emit(it) }
            awaitCancellation()
        }
    }

    private fun noEntries() = emptyList<CoinageTransactionState>()

    private fun entry(
        status: DurableTxStatus,
        claiming: AccountId,
        outputs: Int = 1,
    ) = CoinageTransactionState(
        id = CoinageTransactionId(claiming.value.first().toLong() * 10 + status.ordinal),
        status = status,
        inputs = listOf(CoinageInput.Coin.Received(claiming)),
        outputs = List(outputs) { OwnAsset.Coin(testKey(it)) },
    )

    private companion object {
        val ONE_ASSET = BigInteger.ONE.intoBalance()
        val TWO_ASSETS = BigInteger.TWO.intoBalance()

        val IDLE = 60.seconds

        val RETRY_UNTIL = Instant.fromEpochSeconds(1_000)
        val WINDOW_OPEN = Instant.fromEpochSeconds(500)
        val WINDOW_CLOSED = Instant.fromEpochSeconds(1_500)

        const val DERIVATION_FILE = "io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemoKt"
    }
}
