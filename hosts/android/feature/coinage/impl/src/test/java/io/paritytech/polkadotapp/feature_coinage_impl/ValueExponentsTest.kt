package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.tokenAmount
import io.paritytech.polkadotapp.test_shared.assertEquals
import org.junit.Test
import java.math.BigDecimal

class ValueExponentsTest {
    @Test
    fun `test positive exponent`() {
        testExponent(2, 4.toBigDecimal())
    }

    @Test
    fun `test zero exponent`() {
        testExponent(0, 1.toBigDecimal())
    }

    @Test
    fun `test negative exponent`() {
        testExponent(-2, 0.25.toBigDecimal())
    }

    private fun testExponent(exponent: Int, expectedValue: BigDecimal) {
        val amountResult = ValueExponent(exponent).tokenAmount()
        assertEquals(expectedValue, amountResult)
    }
}
