package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.common.centsToDollar
import io.paritytech.polkadotapp.feature_coinage_impl.common.coinageTestPrecision
import io.paritytech.polkadotapp.feature_coinage_impl.common.testConversionContext
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinAmountBreakdownContext
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigDecimal
import java.math.BigInteger

class DenominationTest {
    private val allowedExponents = (-2..7).map { ValueExponent(it) }.toSet()
    private val coinAmountBreakdown: CoinAmountBreakdown = RealCoinAmountBreakdownContext(coinageTestPrecision, testConversionContext, allowedExponents)

    @Test
    fun `should breakdown exact powers of 2`() {
        testBreakdown(input = 128.0, expected = listOf(7))
        testBreakdown(input = 64.0, expected = listOf(6))
        testBreakdown(input = 32.0, expected = listOf(5))
        testBreakdown(input = 16.0, expected = listOf(4))
        testBreakdown(input = 8.0, expected = listOf(3))
        testBreakdown(input = 4.0, expected = listOf(2))
        testBreakdown(input = 2.0, expected = listOf(1))
        testBreakdown(input = 1.0, expected = listOf(0))
        testBreakdown(input = 0.5, expected = listOf(-1))
        testBreakdown(input = 0.25, expected = listOf(-2))
    }

    @Test
    fun `should breakdown combinations of powers`() {
        testBreakdown(input = 18.5, expected = listOf(4, 1, -1))

        testBreakdown(input = 0.75, expected = listOf(-1, -2))

        testBreakdown(input = 255.75, expected = listOf(7, 6, 5, 4, 3, 2, 1, 0, -1, -2))
    }

    @Test
    fun `should use multiple max denominations for huge numbers`() {
        testBreakdown(input = 256.0, expected = listOf(7, 7))

        testBreakdown(input = 130.0, expected = listOf(7, 1))
    }

    @Test
    fun `should return empty list for zero input`() {
        testBreakdown(input = 0.0, expected = emptyList())
    }

    @Test(expected = Exception::class)
    fun `should crash when input is not a multiple of min exponent`() {
        testBreakdown(0.1, emptyList())
    }

    @Test(expected = Exception::class)
    fun `should crash when input is not a multiple of min exponent 2`() {
        testBreakdown(16.3, listOf(4, -2))
    }

    @Test
    fun `remainder after breakdown should be zero exactly when breakdown succeeds`() {
        val step = BigDecimal("0.0005")

        val mismatch = (0..20_000).firstOrNull { k ->
            val amount = step * BigDecimal(k)
            val breaksDown = runCatching { coinAmountBreakdown.breakdown(amount) }.isSuccess

            coinAmountBreakdown.remainderAfterBreakdown(amount).isZero() != breaksDown
        }

        assertNull("Remainder disagrees with breakdown for k=$mismatch", mismatch)
    }

    @Test
    fun `remainder after breakdown should be the uncovered planks when the amount is not a multiple of the smallest coin`() {
        assertEquals(Balance(BigInteger("500000000000000")), coinAmountBreakdown.remainderAfterBreakdown(BigDecimal("0.163")))
        assertEquals(Balance(BigInteger("1000000000000000")), coinAmountBreakdown.remainderAfterBreakdown(BigDecimal("0.001")))
    }

    @Test(timeout = REMAINDER_TIMEOUT_MS)
    fun `remainder after breakdown should be zero when a 30-digit amount is representable`() {
        val remainder = coinAmountBreakdown.remainderAfterBreakdown(BigDecimal("123456789012345678901234567890"))

        assertTrue(remainder.isZero())
    }

    private fun testBreakdown(input: Double, expected: List<Int>) = runBlocking {
        val denominations = coinAmountBreakdown.breakdown(input.centsToDollar())
            .map { it.value }

        assertEquals(expected, denominations)
    }

    private companion object {
        const val REMAINDER_TIMEOUT_MS = 1_000L
    }
}
