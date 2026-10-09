package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import org.junit.Assert.assertEquals
import org.junit.Test

class FundingAmountInputTest {
    @Test
    fun `the keypad edits a dollar amount of at most two decimals`() {
        val typed = listOf(
            FundingKey.Point,
            FundingKey.Digit('5'),
            FundingKey.Digit('0'),
            FundingKey.Digit('9'),
        ).fold("") { text, key -> text.applying(key) }

        assertEquals("0.50", typed)
    }

    @Test
    fun `a leading zero is replaced and the whole part is capped`() {
        assertEquals("7", "0".applying(FundingKey.Digit('7')))
        assertEquals("1234567", "1234567".applying(FundingKey.Digit('8')))
        assertEquals("12.", "12.".applying(FundingKey.Point))
        assertEquals("12", "12.".applying(FundingKey.Delete))
    }
}
