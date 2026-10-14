package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.VoucherOffboarding
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class EnsureVouchersPaymentStateTest {
    private val recycling: CoinageRecyclingUseCase = mockk()
    private val planner: ExternalPaymentPlanner = mockk()
    private val offboardFactory: OffboardVouchersPaymentState.Factory = mockk()
    private val awaitRecyclingFactory: AwaitRecyclingPaymentState.Factory = mockk()

    private val state = EnsureVouchersPaymentState(PAYMENT, recycling, planner, offboardFactory, awaitRecyclingFactory)

    @Test
    fun `vouchers that already cover the payment go straight to offboarding`() = runBlocking<Unit> {
        val voucher = voucherInRecycler(1, exponent = 4)
        val offboarding: OffboardVouchersPaymentState = mockk()
        coEvery { planner.plan(PAYMENT.amount) } returns
            Result.success(ExternalPaymentPlan.Ready(VoucherOffboarding(listOf(voucher), planks(6))))
        every { offboardFactory.create(PAYMENT, listOf(voucher.ringVrfKeyIndex), planks(6).value) } returns offboarding

        assertSame(offboarding, state.transition().outcome().getOrThrow())
    }

    /** The recycling is submitted under the payment's own group, so a restarted payment finds it again. */
    @Test
    fun `coins to load are recycled under the payment's group and then waited on`() = runBlocking<Unit> {
        val coins = listOf<Coin>(mockk())
        val exact = voucherInRecycler(1, exponent = 1)
        val awaiting: AwaitRecyclingPaymentState = mockk()
        coEvery { planner.plan(PAYMENT.amount) } returns
            Result.success(ExternalPaymentPlan.LoadCoins(coins, exactVouchers = listOf(exact)))
        coEvery { recycling.recycle(coins, ExternalPaymentGroupIds.recycling(PAYMENT.key)) } returns Result.success(Unit)
        every { awaitRecyclingFactory.create(PAYMENT, listOf(exact.ringVrfKeyIndex)) } returns awaiting

        assertSame(awaiting, state.transition().outcome().getOrThrow())
    }

    @Test
    fun `recycling that cannot be submitted fails the payment`() = runBlocking<Unit> {
        coEvery { planner.plan(PAYMENT.amount) } returns
            Result.success(ExternalPaymentPlan.LoadCoins(listOf(mockk()), exactVouchers = emptyList()))
        coEvery { recycling.recycle(any(), any()) } returns Result.failure(IllegalStateException("no chain"))

        assertEquals(
            FailedPaymentState(PAYMENT, EnsureVouchersPaymentState.RECYCLING_SUBMISSION_FAILED),
            state.transition().outcome().getOrThrow(),
        )
    }

    @Test
    fun `a payment nothing can cover fails`() = runBlocking<Unit> {
        coEvery { planner.plan(PAYMENT.amount) } returns
            Result.success(ExternalPaymentPlan.NotEnoughAmount(planks(1), planks(2), planks(9)))

        assertEquals(
            FailedPaymentState(PAYMENT, EnsureVouchersPaymentState.INSUFFICIENT_BALANCE),
            state.transition().outcome().getOrThrow(),
        )
    }

    /** Nothing was submitted yet, so the worker may simply try again once the chain can be read. */
    @Test
    fun `a plan that cannot be made is left for the worker to retry`() = runBlocking<Unit> {
        coEvery { planner.plan(PAYMENT.amount) } returns Result.failure(IllegalStateException("no chain"))

        assertTrue(state.transition().outcome().isFailure)
        coVerify(exactly = 0) { recycling.recycle(any(), any()) }
    }
}
