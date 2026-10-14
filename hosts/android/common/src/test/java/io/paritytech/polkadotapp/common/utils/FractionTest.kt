package io.paritytech.polkadotapp.common.utils

import io.paritytech.polkadotapp.common.utils.Fraction.Companion.toFraction
import io.paritytech.polkadotapp.test_shared.assertEquals
import org.junit.Test

class FractionTest {
    @Test
    fun `should create fraction from big decimal`() {
        testCreateFraction(input = 0.1, expected = 0.1, FractionUnit.FRACTION)
        testCreateFraction(input = 10.0, expected = 0.1, FractionUnit.PERCENT)
        testCreateFraction(input = 123456.0, expected = 0.123456, FractionUnit.PERMILL)
    }

    private fun testCreateFraction(input: Double, expected: Double, unit: FractionUnit) {
        val actual = input.toBigDecimal().toFraction(unit).fraction
        assertEquals(expected.toBigDecimal(), actual)
    }
}
