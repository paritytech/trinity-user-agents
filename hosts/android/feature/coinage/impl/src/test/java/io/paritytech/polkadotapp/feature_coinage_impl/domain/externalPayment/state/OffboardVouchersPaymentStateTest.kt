package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase.ExternalUnloadStatus
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase.UnloadRecyclerIntoExternalAssetUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class OffboardVouchersPaymentStateTest {
    private val voucherRepository: VoucherRepository = mockk()
    private val unload: UnloadRecyclerIntoExternalAssetUseCase = mockk()

    private val voucher = voucherInRecycler(1, exponent = 4)
    private val group = ExternalPaymentGroupIds.unload(PAYMENT.key)

    private val state = OffboardVouchersPaymentState(
        context = PAYMENT,
        selected = listOf(voucher.ringVrfKeyIndex),
        surplusPlanks = planks(6).value,
        voucherRepository = voucherRepository,
        unloadIntoExternalAsset = unload,
    )

    @Test
    fun `the selected vouchers are unloaded under the payment's group and the payment completes with it`() = runBlocking<Unit> {
        givenSubmitted()
        givenUnloadReports(ExternalUnloadStatus.Submitted, ExternalUnloadStatus.FinalizedSuccess)

        assertEquals(CompletedPaymentState(PAYMENT), state.transition().outcome().getOrThrow())
    }

    @Test
    fun `an unload that partly executed reports what reached the destination`() = runBlocking<Unit> {
        givenSubmitted()
        givenUnloadReports(ExternalUnloadStatus.PartialSuccess(executed = 1, total = 2, claimed = planks(4)))

        assertEquals(PartiallyCompletedPaymentState(PAYMENT, planks(4)), state.transition().outcome().getOrThrow())
    }

    @Test
    fun `an unload where nothing executed fails the payment`() = runBlocking<Unit> {
        givenSubmitted()
        givenUnloadReports(ExternalUnloadStatus.Failed)

        assertEquals(
            FailedPaymentState(PAYMENT, OffboardVouchersPaymentState.NOTHING_UNLOADED),
            state.transition().outcome().getOrThrow(),
        )
    }

    @Test
    fun `an unload that cannot be submitted fails the payment`() = runBlocking<Unit> {
        coEvery { voucherRepository.getByRingVrfKeyIndices(listOf(voucher.ringVrfKeyIndex)) } returns listOf(voucher)
        coEvery { unload.initiateUnload(listOf(voucher), PAYMENT.destination, planks(6), group) } returns
            Result.failure(IllegalArgumentException("not in a recycler"))

        assertEquals(
            FailedPaymentState(PAYMENT, OffboardVouchersPaymentState.UNLOAD_SUBMISSION_FAILED),
            state.transition().outcome().getOrThrow(),
        )
    }

    /** The unload is registered, so a retry only rejoins it: the group already holds its transactions. */
    @Test
    fun `losing track of a submitted unload is left for the worker to retry`() = runBlocking<Unit> {
        givenSubmitted()
        every { unload.subscribeUnloadStatus(group) } returns flow { throw IllegalStateException("no database") }

        assertTrue(state.transition().outcome().isFailure)
    }

    private fun givenSubmitted() {
        coEvery { voucherRepository.getByRingVrfKeyIndices(listOf(voucher.ringVrfKeyIndex)) } returns listOf(voucher)
        coEvery { unload.initiateUnload(listOf(voucher), PAYMENT.destination, planks(6), group) } returns Result.success(Unit)
    }

    private fun givenUnloadReports(vararg statuses: ExternalUnloadStatus) {
        every { unload.subscribeUnloadStatus(group) } returns flowOf(*statuses)
    }
}
