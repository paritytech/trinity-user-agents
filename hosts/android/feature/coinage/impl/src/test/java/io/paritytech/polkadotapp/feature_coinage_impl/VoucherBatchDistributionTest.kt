package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.common.centsToDollar
import io.paritytech.polkadotapp.feature_coinage_impl.common.coinageTestPrecision
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinAmountBreakdownContext
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.VoucherBatch
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.VoucherBatchDistribution
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock

class VoucherBatchDistributionTest {
    private val allowedExponents = (-2..7).map { ValueExponent(it) }.toSet()
    private val breakdown: CoinAmountBreakdown = RealCoinAmountBreakdownContext(coinageTestPrecision, testConversionContext, allowedExponents)

    @Test
    fun `recipient larger than any single batch is split across batches - the regression case`() {
        // Two 32c vouchers in different rings: each batch is worth 32c on its own. The recipient needs 64c.
        // The old code broke 64c into a single 2^6=64c coin and could not place it into any 32c batch.
        // The fix fills each batch's value first, so it produces two 32c (2^5) coins instead of one impossible 64c.
        val batches = distribute(
            vouchers = listOf(voucher(exponent = 5, ring = 1), voucher(exponent = 5, ring = 2)),
            recipientCents = 64.0
        )

        assertEquals(
            listOf(
                batch(recipient = listOf(5), change = emptyList()),
                batch(recipient = listOf(5), change = emptyList())
            ),
            batches.recipientAndChangeExponents()
        )
    }

    @Test
    fun `single batch matches the global breakdown`() {
        // One 64c voucher, recipient 48c (32+16). Change is the leftover 16c.
        val batches = distribute(
            vouchers = listOf(voucher(exponent = 6, ring = 1)),
            recipientCents = 48.0
        )

        assertEquals(
            listOf(batch(recipient = listOf(5, 4), change = listOf(4))),
            batches.recipientAndChangeExponents()
        )
    }

    @Test
    fun `recipient spills into a second batch and the rest becomes change`() {
        // Two 32c batches, recipient 40c: first batch fully to recipient (32c), second gives 8c to recipient
        // and keeps 24c (16+8) as change.
        val batches = distribute(
            vouchers = listOf(voucher(exponent = 5, ring = 1), voucher(exponent = 5, ring = 2)),
            recipientCents = 40.0
        )

        assertEquals(
            listOf(
                batch(recipient = listOf(5), change = emptyList()),
                batch(recipient = listOf(3), change = listOf(4, 3))
            ),
            batches.recipientAndChangeExponents()
        )
    }

    @Test
    fun `same-ring vouchers beyond maxConsolidation split into multiple batches`() {
        // Three 32c vouchers in one ring, maxConsolidation 2 → batches of 2 and 1 vouchers (64c and 32c).
        // Recipient 96c: the 2-voucher batch produces a single 64c (2^6) coin, the 1-voucher batch a 32c coin.
        val batches = distribute(
            vouchers = listOf(voucher(exponent = 5, ring = 7), voucher(exponent = 5, ring = 7), voucher(exponent = 5, ring = 7)),
            recipientCents = 96.0,
            maxConsolidation = 2
        )

        assertEquals(listOf(2, 1), batches.map { it.vouchers.size })
        assertEquals(
            listOf(
                batch(recipient = listOf(6), change = emptyList()),
                batch(recipient = listOf(5), change = emptyList())
            ),
            batches.recipientAndChangeExponents()
        )
    }

    private fun distribute(
        vouchers: List<RecyclerVoucher>,
        recipientCents: Double,
        maxConsolidation: Int = 64
    ): List<VoucherBatch> = VoucherBatchDistribution.distribute(
        vouchers = vouchers,
        recipientAmount = recipientCents.centsToDollar(),
        maxConsolidation = maxConsolidation,
        breakdown = breakdown,
        conversionContext = testConversionContext
    )

    private fun batch(recipient: List<Int>, change: List<Int>): Pair<List<Int>, List<Int>> = recipient to change

    private fun List<VoucherBatch>.recipientAndChangeExponents(): List<Pair<List<Int>, List<Int>>> =
        map { it.recipientDenominations.values() to it.changeDenominations.values() }

    private fun List<ValueExponent>.values(): List<Int> = map { it.value }

    private var voucherIndexCounter = 0

    private fun voucher(exponent: Int, ring: Int): RecyclerVoucher = RecyclerVoucher(
        ringVrfKeyIndex = testKey(voucherIndexCounter++),
        ringVrfPublicKey = mock(),
        recyclerValue = ValueExponent(exponent),
        location = Location.InRecycler(RingIndex(ring.toBigInteger()), recyclerMembers = FULL_RING, enteredAt = null),
        recyclerFungibility = RecyclerFungibility.NONE,
        maxRecyclerFungibility = RecyclerFungibility.NONE,
    )

    private companion object {
        const val FULL_RING = 767
    }
}
