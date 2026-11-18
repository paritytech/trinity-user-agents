package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferCoinEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealSendEnterAmountInteractorTest {
    private val recipient: AccountId = byteArrayOf(1).intoAccountId()

    private val memo = TransferMemo(
        coins = listOf(TransferCoinEntry(byteArrayOf(2).toDataByteArray(), ValueExponent(0))),
        totalValue = 1.intoBalance(),
    )

    private val statusUseCase = mockk<CoinagePaymentStatusUseCase>()
    private val handoffCommit = mockk<CoinageHandoffCommit> {
        coEvery { commit() } returns Result.success(Unit)
        coEvery { release() } returns Result.success(Unit)
    }
    private val submitter = mockk<CoinsSubmitter> {
        coEvery { submit(any(), any(), any()) } returns Result.success(Unit)
    }
    private val sentMessageId = slot<String>()
    private val prepareUseCase = RecordingPrepareUseCase()

    @Test
    fun `a claim seen at the best block completes the payment`() = runTest {
        givenStatuses(listOf(AwaitingClaim), listOf(Claimed(finalized = false)))

        val states = sendViaSubmitter()

        assertEquals(listOf(SendState.Detected, SendState.Complete(prepareUseCase.exportId)), states)
    }

    @Test
    fun `an empty first reading keeps watching`() = runTest {
        givenStatuses(emptyList(), listOf(AwaitingClaim), listOf(Claimed(finalized = false)))

        val states = sendViaSubmitter()

        assertEquals(
            listOf(SendState.Detecting, SendState.Detected, SendState.Complete(prepareUseCase.exportId)),
            states
        )
    }

    @Test
    fun `the submitter gets the keys only after the handoff is committed`() = runTest {
        givenStatuses(listOf(Claimed(finalized = false)))

        sendViaSubmitter()

        coVerifyOrder {
            handoffCommit.commit()
            submitter.submit(memo, BigDecimal.ONE, any())
        }
    }

    @Test
    fun `a chat payment is followed by the id of the message carrying it`() = runTest {
        val states = sendViaChat()

        assertEquals(listOf(SendState.Complete(unfinalizedExport = null)), states)
        assertEquals(CoinageExportId.ofChatPayment(sentMessageId.captured), prepareUseCase.exportId)
    }

    @Test
    fun `fails the send and takes the coins back when the chat handoff commit is refused`() = runTest {
        coEvery { handoffCommit.commit() } returns Result.failure(IllegalStateException("handed off meanwhile"))

        val states = sendViaChat()

        assertTrue(states.single() is SendState.Failed)
        coVerify { handoffCommit.release() }
    }

    private fun givenStatuses(vararg readings: List<CoinagePaymentStatus>) {
        every { statusUseCase.subscribeStatuses(any()) } returns flow {
            readings.forEach { statuses ->
                emit(statuses.map { CoinagePaymentState(recipient, Balance.ONE, it) })
            }
            // A storage subscription stays open, so only the interactor can end the watch.
            awaitCancellation()
        }
    }

    private suspend fun TestScope.sendViaSubmitter(): List<SendState> =
        send(TransferMethod.CoinsViaSubmitter(SUBMITTER_ID, byteArrayOf()))

    private suspend fun TestScope.sendViaChat(): List<SendState> =
        send(TransferMethod.CoinsViaChat(recipient))

    private suspend fun TestScope.send(method: TransferMethod): List<SendState> {
        val interactor = interactor(StandardTestDispatcher(testScheduler))

        return with(StalenessReportCollector.NoOp) { interactor.send(BigDecimal.ONE, method) }.toList()
    }

    private fun interactor(dispatcher: CoroutineDispatcher) = RealSendEnterAmountInteractor(
        chainAssetProvider = mockk { coEvery { asset() } returns mockk(relaxed = true) },
        transfersTypeRegistry = mockk(),
        sendRecipientRepository = mockk { coEvery { addSendRecipient(any()) } returns Result.success(Unit) },
        freeTransactionOrigins = mockk(),
        chatMessageSender = chatMessageSender(),
        prepareCoinageTransferUseCase = prepareUseCase,
        totalBalanceUseCase = mockk(),
        externalPaymentService = mockk(),
        externalPaymentPlanner = mockk(),
        coinsSubmitters = mapOf(SUBMITTER_ID to submitter),
        coinagePaymentStatusUseCase = statusUseCase,
        coinageDebugSettings = mockk(),
        coroutineDispatchers = mockk<CoroutineDispatchers> { every { computation } returns dispatcher },
        timeProvider = mockk { every { now() } returns Instant.fromEpochMilliseconds(0) },
        sendValidation = mockk(),
    )

    // Runs onSaved as the message transaction does, so whatever it throws leaves the send as it would at runtime.
    private fun chatMessageSender() = mockk<ChatMessageSender> {
        coEvery { sendUserMessage(capture(sentMessageId), any(), any(), any(), any()) } coAnswers {
            arg<suspend () -> Unit>(4).invoke()
            mockk()
        }
    }

    private inner class RecordingPrepareUseCase : PrepareCoinageTransferUseCase {
        var exportId: CoinageExportId? = null

        override suspend fun preparePlan(amount: BigDecimal) =
            Result.success(TransferPlan(amount, spendsGainingPrivacy = false))

        @OptIn(ExperimentalTime::class)
        override suspend fun prepareMemo(plan: TransferPlan, exportId: CoinageExportId, retryUntil: Instant): Result<PreparedTransferMemo> {
            this.exportId = exportId
            return Result.success(PreparedTransferMemo(exportId, memo, handoffCommit))
        }
    }

    private companion object {
        const val SUBMITTER_ID = "test"
    }
}
