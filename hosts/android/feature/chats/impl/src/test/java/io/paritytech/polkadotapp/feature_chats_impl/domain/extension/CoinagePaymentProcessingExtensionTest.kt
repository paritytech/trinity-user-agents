package io.paritytech.polkadotapp.feature_chats_impl.domain.extension

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.asFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Before
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration.Companion.days
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * When a payment message is done with.
 *
 * Marking a message processed is what stops the app ever looking at it again, so it is the point of no
 * return for money someone has already sent us. Everything here is about not reaching it early: an incoming
 * payment is finished only when the claim use case says so, and a claim that reported a partial amount
 * along the way has said nothing about being finished.
 */
@OptIn(ExperimentalTime::class)
class CoinagePaymentProcessingExtensionTest {
    private val paymentStatusUseCase: CoinagePaymentStatusUseCase = mockk()
    private val claimReceivedCoinsUseCase: ClaimReceivedCoinsUseCase = mockk()
    private val balanceConverterUseCase: CoinageBalanceConverterUseCase = mockk()

    private val extension = CoinagePaymentProcessingExtension(
        paymentStatusUseCase = paymentStatusUseCase,
        claimReceivedCoinsUseCase = claimReceivedCoinsUseCase,
        balanceConverterUseCase = balanceConverterUseCase,
    )

    /** The outgoing path derives a keypair per coin key; only the seam matters, and the real one needs JNI. */
    @Before
    fun mockDerivation() {
        mockkStatic(DERIVATION_FILE)
        every { any<CoinPrivateKey>().deriveKeypair() } answers {
            val key = firstArg<CoinPrivateKey>()
            mockk<Sr25519Keypair> { every { publicKey } returns key.value }
        }
    }

    @After
    fun unmockDerivation() = unmockkStatic(DERIVATION_FILE)

    private val context: ChatExtensionContext = mockk(relaxed = true)
    private lateinit var scope: CoroutineScope

    @After
    fun stopWork() {
        if (::scope.isInitialized) scope.cancel()
    }

    /**
     * The claim reported an amount and is still working — a coin has not been claimed yet, and retrying is
     * still allowed. Closing the message here is how a 1.00 payment was settled as 0.12: the partial amount
     * was reported, taken as final, and nothing looked at the message again.
     */
    @Test
    fun `an incoming payment is not closed while its claim is still working`() = runTest {
        givenClaimReports(CoinageTransferDetection.ClaimingRest(PARTIAL)) { awaitCancellation() }

        startWork(incomingPayment())

        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * The claim use case owns when to stop: it retries until every coin is claimed or the caller's window
     * closes, so its completion — not any status it reported on the way — is what finishes the message.
     */
    @Test
    fun `an incoming payment is closed once its claim is done`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claimed(FULL, finalized = true))

        val message = incomingPayment()
        startWork(message)

        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * Every claim failed and the window has closed, so the use case gave up. The message is finished even
     * though no money arrived — there is nothing left to try, and leaving it open would retry it forever.
     */
    @Test
    fun `an incoming payment whose claims all failed is closed once the claim gives up`() = runTest {
        givenClaimReports(CoinageTransferDetection.NotClaimed)

        val message = incomingPayment()
        startWork(message)

        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * A payment whose coins never land would otherwise keep a claim and its chain connection alive forever,
     * across every restart. The deadline counts from the message, so reopening the app cannot push it back.
     */
    @Test
    fun `a claim waits a week from the payment for coins to appear`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claimed(FULL, finalized = true))
        val message = incomingPayment()

        startWork(message)

        val sentAt = Instant.fromEpochMilliseconds(message.timestamp)
        coVerify { claimReceivedCoinsUseCase.claim(any(), any(), sentAt + 7.days) }
    }

    /** The group is the message's own, so a retry rejoins the claims an earlier attempt registered. */
    @Test
    fun `claims are registered under a group derived from the message`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claimed(FULL, finalized = true))

        val message = incomingPayment()
        startWork(message)

        coVerify { claimReceivedCoinsUseCase.claim(any(), CoinageOperationGroupId("chat-claim:${message.id}"), any()) }
    }

    /** What the claim reports on the way is shown, whether or not it is the last thing it will say. */
    @Test
    fun `the message shows what the claim reports as it goes`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claiming)

        val message = incomingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detected(FULL) },
            )
        }
    }

    /** A payment held up by a failed claim shows how much of it is already the user's. */
    @Test
    fun `a payment held up by a failed claim shows what has landed`() = runTest {
        givenClaimReports(CoinageTransferDetection.ClaimingRest(PARTIAL)) { awaitCancellation() }

        val message = incomingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.PartiallyClaimed(PARTIAL) },
            )
        }
    }

    /**
     * An app killed mid-claim: the scope goes and the flow never completes. The message stays unprocessed so
     * the next launch picks it up again, which is the only thing that makes a claim survive a restart.
     */
    @Test
    fun `a claim cut short by shutdown leaves the payment open`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claiming) { awaitCancellation() }

        startWork(incomingPayment())
        scope.cancel()

        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * Claiming ended with only part of the payment ours. The message reports what landed, which is what the
     * bubble compares against the amount sent to show the shortfall — the one place that comparison is
     * allowed to happen.
     */
    @Test
    fun `an incoming payment that ended short reports what landed`() = runTest {
        givenClaimReports(CoinageTransferDetection.ClaimedPartially(PARTIAL))

        val message = incomingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Transferred(PARTIAL) },
            )
        }
        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    @Test
    fun `an incoming payment reports detecting before anything is found`() = runTest {
        givenClaimReports(CoinageTransferDetection.Detecting) { awaitCancellation() }

        val message = incomingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detecting },
            )
        }
    }

    /** A payment that lands while the app is open is picked up from the live feed, not only at startup. */
    @Test
    fun `a payment arriving while the app is running is processed`() = runTest {
        givenClaimReports(CoinageTransferDetection.Claimed(FULL, finalized = true))

        val message = incomingPayment()
        startWork(arriving = listOf(message))

        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    // ---- outgoing ----

    /**
     * The peer has taken some of the coins and the rest are still sitting on chain.
     *
     * The sender is told the payment is awaiting claim, for the whole amount. PartiallyClaimed belongs to
     * claiming, which only the receiving side does — a sender reporting "claimed 0.12 of 1.00" would be
     * announcing a shortfall that is entirely the peer's to resolve, and that resolves itself the moment
     * they claim the rest.
     */
    @Test
    fun `an outgoing payment part-taken by the peer still reads as awaiting claim`() = runTest {
        givenPeerHasTaken(claimed = 1, awaiting = 1, detecting = 0, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.Detected },
            )
        }
        coVerify(exactly = 0) {
            context.modifyMessage(
                any(),
                any(),
                match { it is CoinagePayment && it.status is CoinagePayment.Status.PartiallyClaimed },
            )
        }
    }

    /** And it is not closed out while the peer can still take the rest. */
    @Test
    fun `an outgoing payment is not closed while coins are still awaiting claim`() = runTest {
        givenPeerHasTaken(claimed = 1, awaiting = 1, detecting = 0, failed = 0, finalized = true)

        startWork(outgoingPayment())

        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /** Nothing the sender minted is on chain yet, so the payment still reads as being sent. */
    @Test
    fun `an outgoing payment whose coins have not appeared yet still reads as sending`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 2, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detecting },
            )
        }
    }

    /**
     * A large payment is minted by several transactions, so its coins appear on chain in waves. A quick peer
     * can take every coin that exists while the rest are still being minted.
     *
     * The payment is not over: reporting it as transferred for what has been taken so far makes the bubble
     * compare a fraction against the amount sent and announce a shortfall — a 35.1 payment read as "claimed
     * 2.xx" for the half-minute before the remaining transactions landed.
     */
    @Test
    fun `an outgoing payment is not called transferred while coins are still being minted`() = runTest {
        givenPeerHasTaken(claimed = 2, awaiting = 0, detecting = 3, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify(exactly = 0) {
            context.modifyMessage(
                any(),
                any(),
                match { it is CoinagePayment && it.status is CoinagePayment.Status.Transferred },
            )
        }
        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * Nothing is pending any more: what was minted has been taken and the rest never existed. That really is
     * the end, and the shortfall is real.
     */
    @Test
    fun `an outgoing payment settles short when the coins that failed will never exist`() = runTest {
        givenPeerHasTaken(claimed = 2, awaiting = 0, detecting = 0, failed = 3, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.Transferred },
            )
        }
        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * The peer has taken every coin, as far as the best chain shows, but no claim has finalized.
     *
     * Both halves matter and they differ. The message says so at once — making the sender wait a whole
     * finality to be told their payment landed is the latency this design exists to avoid. But it stays
     * open, because a fork can still put those coins back and nothing watches a closed message.
     */
    @Test
    fun `an outgoing payment reads as claimed at once but is not closed until that is beyond recall`() = runTest {
        givenPeerHasTaken(claimed = 2, awaiting = 0, detecting = 0, failed = 0, finalized = false)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.Transferred },
            )
        }
        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * Some coins were never minted and the rest have been taken, but only as far as the best chain shows.
     *
     * The two rules pull opposite ways here, and both are right: the shortfall is displayed, because coins
     * that failed to mint are never coming back and the amount really is short; and the message stays open,
     * because a fork could still return the claimed ones and change what that shortfall is.
     */
    @Test
    fun `an outgoing payment short of failed coins shows the shortfall while claims can still be rolled back`() =
        runTest {
            givenPeerHasTaken(claimed = 2, awaiting = 0, detecting = 0, failed = 3, finalized = false)

            val message = outgoingPayment()
            startWork(message)

            coVerify {
                context.modifyMessage(
                    message.chatId,
                    message.id,
                    match { it is CoinagePayment && it.status is CoinagePayment.Status.Transferred },
                )
            }
            coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
        }

    /** Nothing has been taken yet and some coins never will be: still sending, for the ones that can arrive. */
    @Test
    fun `an outgoing payment still being minted alongside coins that failed reads as sending`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 2, failed = 3, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detecting },
            )
        }
        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * Nothing is known about the payment's coins yet — no rows to read.
     *
     * That is ignorance, not completion. Closing on it settles a payment for good on the one reading that
     * says nothing at all, and nothing watches a closed message afterwards.
     */
    @Test
    fun `an outgoing payment is not closed while nothing is known about its coins`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 0, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detecting },
            )
        }
    }

    /** Every coin taken: the payment is done and the message never needs looking at again. */
    @Test
    fun `an outgoing payment is closed once the peer has taken every coin`() = runTest {
        givenPeerHasTaken(claimed = 2, awaiting = 0, detecting = 0, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.Transferred },
            )
        }
        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * The coins were never minted, so the keys the peer holds control nothing. Nothing will change that, so
     * the message is closed rather than watched forever.
     */
    @Test
    fun `an outgoing payment whose coins never existed is closed as failed`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 0, failed = 2, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.FailedDetection },
            )
        }
        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * The payment is saved, and its transactions are waiting to be built — or to be built again after an
     * attempt that could never land. Coinage reports a coin whose mint waits like that as still detecting.
     */
    @Test
    fun `an outgoing payment whose transactions are still being built reads as sending`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 2, failed = 0, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detecting },
            )
        }
    }

    /** A rebuild can take hours while the payment's inputs are missing, and the message has to be watched throughout. */
    @Test
    fun `an outgoing payment is not closed while its transactions await submission`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 2, failed = 0, finalized = true)

        startWork(outgoingPayment())

        coVerify(exactly = 0) { context.markMessageProcessed(any(), any()) }
    }

    /**
     * The window for rebuilding closed with the payment's inputs gone from the chain, so its coins will never
     * be minted. Coinage reports that as failed, and the message is closed on it.
     */
    @Test
    fun `an outgoing payment whose build gave up is closed as failed`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 0, detecting = 0, failed = 2, finalized = true)

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status is CoinagePayment.Status.FailedDetection },
            )
        }
        coVerify(exactly = 1) { context.markMessageProcessed(message.chatId, message.id) }
    }

    /**
     * The coins cannot be valued — the conversion needs chain metadata, which may not be to hand.
     *
     * The payment still reports its stage, with an amount of zero. Failing to say anything would leave the
     * sender staring at a message that never moves off "sending".
     */
    @Test
    fun `an outgoing payment still reports its stage when the coins cannot be valued`() = runTest {
        givenPeerHasTaken(claimed = 0, awaiting = 2, detecting = 0, failed = 0, finalized = true)
        coEvery { balanceConverterUseCase.create() } returns Result.failure(IllegalStateException("no metadata"))

        val message = outgoingPayment()
        startWork(message)

        coVerify {
            context.modifyMessage(
                message.chatId,
                message.id,
                match { it is CoinagePayment && it.status == CoinagePayment.Status.Detected(Balance.ZERO) },
            )
        }
    }

    // ---- harness ----

    private fun startWork(vararg unprocessed: ChatMessage, arriving: List<ChatMessage> = emptyList()) {
        scope = CoroutineScope(UnconfinedTestDispatcher())

        every { context.scope } returns TestComputationalScope(scope)
        every { context.subscribeNewMessages(any(), any()) } returns arriving.asFlow()
        coEvery { context.getUnprocessedMessages(any(), any()) } returns unprocessed.toList()

        with(context) { extension.startGlobalWork() }
    }

    private fun givenClaimReports(
        vararg reports: CoinageTransferDetection,
        andThen: suspend () -> Unit = {},
    ) {
        every { claimReceivedCoinsUseCase.claim(any(), any(), any()) } returns flow {
            reports.forEach { emit(it) }
            andThen()
        }
    }

    /**
     * The whole mix of coins behind one payment, none of it defaulted.
     *
     * What the message shows and whether it may be closed are decided by *which kinds* are present, and the
     * interesting cases are the mixtures. A default would let a test leave one kind unstated and quietly
     * land in a different case than the one it was written for.
     */
    private fun givenPeerHasTaken(
        claimed: Int,
        awaiting: Int,
        detecting: Int,
        failed: Int,
        finalized: Boolean,
    ) {
        val states = buildMap {
            repeat(claimed) { put(accountId(it), stateOf(CoinagePaymentStatus.Claimed(finalized))) }
            repeat(awaiting) { put(accountId(claimed + it), stateOf(CoinagePaymentStatus.AwaitingClaim)) }
            repeat(failed) { put(accountId(claimed + awaiting + it), stateOf(CoinagePaymentStatus.Failed)) }
            repeat(detecting) {
                put(accountId(claimed + awaiting + failed + it), stateOf(CoinagePaymentStatus.Detecting))
            }
        }

        every { paymentStatusUseCase.subscribeStatuses(any()) } returns flow {
            emit(states)
            awaitCancellation()
        }
        // formatCoinsToBalance is a top-level extension over this member, so stubbing the member is enough.
        coEvery { balanceConverterUseCase.create() } returns Result.success(
            mockk { every { formatExponentToBalance(any()) } returns PARTIAL }
        )
    }

    private fun accountId(seed: Int): AccountId = byteArrayOf(seed.toByte()).toDataByteArray()

    private fun stateOf(status: CoinagePaymentStatus) = CoinagePaymentState(
        coin = Coin(derivationIndex = CoinageKeyIndex(CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES).toDataByteArray()), 0), valueExponent = ValueExponent(3), age = Coin.Age.Unknown, isOnChain = false, accountId = accountId(0), provenance = CoinProvenance.UNKNOWN),
        status = status,
    )

    private fun outgoingPayment() = incomingPayment().copy(origin = ChatMessageOrigin.User)

    private fun incomingPayment() = ChatMessage(
        id = "message-1",
        chatId = ChatId.fromContact(byteArrayOf(9).intoAccountId()),
        timestamp = SENT_AT,
        origin = ChatMessageOrigin.Contact(byteArrayOf(9).intoAccountId()),
        content = CoinagePayment(
            totalValue = FULL,
            coinKeys = listOf(byteArrayOf(1), byteArrayOf(2)),
            status = CoinagePayment.Status.Detecting,
        ),
        status = ChatMessage.Status.IS_SENT,
    )

    /**
     * [ComputationalScope]'s own factory returns a value class, which a mocked getter hands back erased to
     * the scope it wraps. A plain implementation survives the boundary.
     */
    private class TestComputationalScope(scope: CoroutineScope) : ComputationalScope, CoroutineScope by scope

    private companion object {
        const val SENT_AT = 1_700_000_000_000L
        const val DERIVATION_FILE = "io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemoKt"

        val FULL = BigInteger.valueOf(100).intoBalance()
        val PARTIAL = BigInteger.valueOf(12).intoBalance()
    }
}
