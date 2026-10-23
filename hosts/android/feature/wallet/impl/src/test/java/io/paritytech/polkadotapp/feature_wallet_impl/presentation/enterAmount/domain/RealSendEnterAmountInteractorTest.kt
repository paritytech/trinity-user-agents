package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.NoProductChatRoomException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatPayments
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferCoinEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.deriveKeypair
import io.paritytech.polkadotapp.feature_coinage_api.domain.submitter.CoinsSubmitter
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus.AwaitingClaim
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus.Claimed
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PrepareCoinageTransferUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PreparedTransferMemo
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.TransferMethod
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealSendEnterAmountInteractorTest {
    private val coinKey: CoinPrivateKey = byteArrayOf(1).toDataByteArray()
    private val accountId: AccountId = coinKey.value.intoAccountId()

    private val memo = TransferMemo(
        coins = listOf(TransferCoinEntry(coinKey, ValueExponent(0))),
        totalValue = 1.intoBalance(),
    )

    private val statusUseCase = mockk<CoinagePaymentStatusUseCase>()
    private var productChat = false
    private val productPayments = RecordingProductPayments()
    private val chatMessageSender = chatMessageSender()

    @Before
    fun mockDerivation() {
        mockkStatic(DERIVATION_FILE)
        val keypair = mockk<Sr25519Keypair> { every { publicKey } returns coinKey.value }
        every { coinKey.deriveKeypair() } returns keypair
    }

    @After
    fun unmockDerivation() = unmockkStatic(DERIVATION_FILE)

    @Test
    fun `a claim seen at the best block completes the payment`() = runTest {
        givenStatuses(listOf(AwaitingClaim), listOf(Claimed(finalized = false)))

        assertEquals(listOf(SendState.Detected, SendState.Complete(listOf(accountId))), sendViaSubmitter())
    }

    @Test
    fun `an empty first reading keeps watching`() = runTest {
        givenStatuses(emptyList(), listOf(AwaitingClaim), listOf(Claimed(finalized = false)))

        assertEquals(listOf(SendState.Detecting, SendState.Detected, SendState.Complete(listOf(accountId))), sendViaSubmitter())
    }

    @Test
    fun `fails the send when the chat handoff commit is refused`() = runTest {
        val refused = mockk<CoinageHandoffCommit> {
            coEvery { commit() } returns Result.failure(IllegalStateException("handed off meanwhile"))
        }

        val states = sendViaChat(refused)

        assertTrue(states.single() is SendState.Failed)
    }

    @Test
    fun `with the chat product the payment goes to the contact's room, committed inside its record`() = runTest {
        productChat = true
        val handoff = committingHandoff()

        val states = sendViaChat(handoff)

        assertEquals(listOf(SendState.Complete(unfinalizedCoins = null)), states)
        val sent = productPayments.sent.single()
        assertEquals(accountId, sent.recipient)
        assertEquals(memo.totalValue, sent.totalValue)
        assertArrayEquals(coinKey.value, sent.coinKeys.single())
        coVerify(exactly = 1) { handoff.commit() }
        coVerify(exactly = 0) { chatMessageSender.sendUserMessage(any(), any(), any(), any(), any()) }
    }

    @Test
    fun `with the chat product a refused commit fails the send`() = runTest {
        productChat = true
        val refused = mockk<CoinageHandoffCommit> {
            coEvery { commit() } returns Result.failure(IllegalStateException("handed off meanwhile"))
        }

        val states = sendViaChat(refused)

        assertTrue(states.single() is SendState.Failed)
    }

    @Test
    fun `with the chat product and no established chat with the recipient the send fails without a commit`() = runTest {
        productChat = true
        productPayments.noRoom = true
        val handoff = committingHandoff()

        val states = sendViaChat(handoff)

        assertTrue((states.single() as SendState.Failed).error is NoProductChatRoomException)
        coVerify(exactly = 0) { handoff.commit() }
    }

    private fun givenStatuses(vararg readings: List<CoinagePaymentStatus>) {
        every { statusUseCase.subscribeStatuses(any()) } returns flow {
            readings.forEach { statuses ->
                emit(statuses.associate { accountId to CoinagePaymentState(mockk(), it) })
            }
            // A storage subscription stays open, so only the interactor can end the watch.
            awaitCancellation()
        }
    }

    private suspend fun TestScope.sendViaSubmitter(): List<SendState> =
        send(TransferMethod.CoinsViaSubmitter(SUBMITTER_ID, byteArrayOf()), committingHandoff())

    private suspend fun TestScope.sendViaChat(handoffCommit: CoinageHandoffCommit): List<SendState> =
        send(TransferMethod.CoinsViaChat(accountId), handoffCommit)

    private suspend fun TestScope.send(method: TransferMethod, handoffCommit: CoinageHandoffCommit): List<SendState> {
        val interactor = interactor(StandardTestDispatcher(testScheduler), handoffCommit)

        return with(StalenessReportCollector.NoOp) { interactor.send(BigDecimal.ONE, method) }.toList()
    }

    private fun interactor(
        dispatcher: CoroutineDispatcher,
        handoffCommit: CoinageHandoffCommit,
    ) = RealSendEnterAmountInteractor(
        chainAssetProvider = mockk { coEvery { asset() } returns mockk<Chain.Asset>(relaxed = true) },
        transfersTypeRegistry = mockk(),
        sendRecipientRepository = mockk { coEvery { addSendRecipient(any()) } returns Result.success(Unit) },
        freeTransactionOrigins = mockk(),
        chatMessageSender = chatMessageSender,
        personChatMode = mockk<PersonChatMode> { every { isProductChat() } answers { productChat } },
        productChatPayments = productPayments,
        prepareCoinageTransferUseCase = prepareUseCase(handoffCommit),
        totalBalanceUseCase = mockk(),
        externalPaymentService = mockk(),
        externalPaymentPlanner = mockk(),
        coinsSubmitters = mapOf(SUBMITTER_ID to submitter()),
        coinagePaymentStatusUseCase = statusUseCase,
        coinageDebugSettings = mockk(),
        coroutineDispatchers = mockk<CoroutineDispatchers> { every { computation } returns dispatcher },
        timeProvider = mockk { every { now() } returns Instant.fromEpochMilliseconds(0) },
        sendValidation = mockk(),
    )

    // Runs onSaved as the message transaction does, so whatever it throws leaves the send as it would at runtime.
    private fun chatMessageSender() = mockk<ChatMessageSender> {
        coEvery { sendUserMessage(any(), any(), any(), any(), any()) } coAnswers {
            arg<suspend () -> Unit>(4).invoke()
            mockk()
        }
    }

    // Runs onSaved as the record's transaction does, so whatever it throws leaves the send as it would at runtime.
    private class RecordingProductPayments : ProductChatPayments {
        class Sent(val recipient: AccountId, val totalValue: Balance, val coinKeys: List<ByteArray>)

        val sent = mutableListOf<Sent>()
        var noRoom = false

        override suspend fun send(recipient: AccountId, totalValue: Balance, coinKeys: List<ByteArray>, onSaved: suspend () -> Unit) {
            if (noRoom) throw NoProductChatRoomException()
            onSaved()
            sent += Sent(recipient, totalValue, coinKeys)
        }

        override suspend fun receive(chatId: ChatId, messageId: String, timestamp: Long, totalValue: Balance, coinKeys: List<ByteArray>) = Unit
    }

    private fun committingHandoff() = mockk<CoinageHandoffCommit> { coEvery { commit() } returns Result.success(Unit) }

    private fun submitter() = mockk<CoinsSubmitter> {
        coEvery { submit(any(), any(), any()) } returns Result.success(Unit)
    }

    // mockk resolves a Result return through kotlin-reflect, which rejects context-parameter functions.
    private fun prepareUseCase(handoffCommit: CoinageHandoffCommit) = object : PrepareCoinageTransferUseCase {
        private val plan = TransferPlan(StrategyType.ExactCoins(emptyList()))

        override suspend fun preparePlan(amount: BigDecimal) = Result.success(plan)

        @OptIn(ExperimentalTime::class)
        context(diagnostics: StalenessReportCollector)
        override suspend fun prepareMemo(plan: TransferPlan, retryUntil: Instant) =
            Result.success(PreparedTransferMemo(memo, handoffCommit))

        @OptIn(ExperimentalTime::class)
        context(diagnostics: StalenessReportCollector)
        override suspend fun prepareScheduledMemo(plan: TransferPlan, retryUntil: Instant) =
            Result.success(PreparedTransferMemo(memo, handoffCommit))
    }

    private companion object {
        const val SUBMITTER_ID = "test"
        const val DERIVATION_FILE = "io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemoKt"
    }
}
