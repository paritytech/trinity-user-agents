package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.VoucherOffboarding
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageRecyclingUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.PowerOfTwoConversion
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.ExternalPaymentGroupIds
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

/** The payment's [PAYMENT] amount is 10 planks; a voucher of exponent `e` is worth 2^e. */
class AwaitRecyclingPaymentStateTest {
    private val recycling: CoinageRecyclingUseCase = mockk()
    private val voucherRepository: VoucherRepository = mockk()
    private val converter: CoinageBalanceConverterUseCase = mockk {
        coEvery { create() } returns Result.success(PowerOfTwoConversion)
    }
    private val planner: ExternalPaymentPlanner = mockk()
    private val offboardFactory: OffboardVouchersPaymentState.Factory = mockk()

    private val exact = voucherInRecycler(1, exponent = 1)

    private val state = AwaitRecyclingPaymentState(
        context = PAYMENT,
        exactVouchers = listOf(exact.ringVrfKeyIndex),
        coinageRecyclingUseCase = recycling,
        voucherRepository = voucherRepository,
        coinageBalanceConverterUseCase = converter,
        externalPaymentPlanner = planner,
        offboardFactory = offboardFactory,
    )

    /** Best-block recycling is enough to go on: finality is not waited for. */
    @Test
    fun `recycled vouchers are offboarded together with the exact ones once recycling reaches the best block`() = runBlocking<Unit> {
        val recycled = voucherInRecycler(2, exponent = 3)
        val offboarding: OffboardVouchersPaymentState = mockk()
        givenRecycling(RecyclingStatus.Pending, RecyclingStatus.AllRecycled(listOf(recycled), finalized = false))
        givenExactVouchers(exact)
        coEvery { planner.pickOffboarding(listOf(recycled, exact), PAYMENT.amount) } returns
            Result.success(VoucherOffboarding(listOf(recycled, exact), planks(0)))
        every { offboardFactory.create(PAYMENT, listOf(recycled.ringVrfKeyIndex, exact.ringVrfKeyIndex), planks(0).value) } returns
            offboarding

        assertSame(offboarding, state.transition().outcome().getOrThrow())
    }

    @Test
    fun `recycling that will never complete fails the payment`() = runBlocking<Unit> {
        givenRecycling(RecyclingStatus.Pending, RecyclingStatus.Incomplete)

        assertEquals(
            FailedPaymentState(PAYMENT, AwaitRecyclingPaymentState.RECYCLING_INCOMPLETE),
            state.transition().outcome().getOrThrow(),
        )
    }

    /**
     * An exact voucher was spent by something else while the coins were recycling, so what is left no longer
     * covers the payment. Nothing is unloaded rather than unloading less than was asked for.
     */
    @Test
    fun `recycled and exact vouchers that no longer cover the payment fail it`() = runBlocking<Unit> {
        givenRecycling(RecyclingStatus.AllRecycled(listOf(voucherInRecycler(2, exponent = 3)), finalized = false))
        givenExactVouchers()

        assertEquals(
            FailedPaymentState(PAYMENT, AwaitRecyclingPaymentState.INSUFFICIENT_AFTER_RECYCLING),
            state.transition().outcome().getOrThrow(),
        )
        coVerify(exactly = 0) { planner.pickOffboarding(any(), any()) }
    }

    private fun givenRecycling(vararg statuses: RecyclingStatus) {
        every { recycling.observeRecyclingStatus(ExternalPaymentGroupIds.recycling(PAYMENT.key)) } returns flowOf(*statuses)
    }

    private fun givenExactVouchers(vararg vouchers: RecyclerVoucher) {
        coEvery { voucherRepository.getByRingVrfKeyIndices(listOf(exact.ringVrfKeyIndex)) } returns vouchers.toList()
    }
}
