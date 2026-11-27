package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FundingBalanceUnitsTest {
    @Test
    fun `the core's units read as a balance`() {
        assertEquals(50_200_000.intoBalance(), "50200000".toBalanceOrNull())
        val u128Max = "340282366920938463463374607431768211455"
        assertEquals(u128Max.toBigInteger().intoBalance(), u128Max.toBalanceOrNull())
    }

    @Test
    fun `units that are no number read as none`() {
        assertNull("not a number".toBalanceOrNull())
        assertNull("".toBalanceOrNull())
    }

    @Test
    fun `a balance goes back to the core as the same units`() {
        assertEquals("50200000", 50_200_000.intoBalance().toU128())
        assertEquals("0", 0.intoBalance().toU128())
    }
}
