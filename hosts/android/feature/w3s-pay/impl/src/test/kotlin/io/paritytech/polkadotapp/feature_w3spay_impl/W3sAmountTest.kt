package io.paritytech.polkadotapp.feature_w3spay_impl

import io.paritytech.polkadotapp.feature_w3spay_impl.domain.parseW3sDecimalAmount
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.math.BigDecimal

class W3sAmountTest {
    @Test
    fun `accepts integer and two-decimal amounts`() {
        assertEquals(0, BigDecimal("9").compareTo(parseW3sDecimalAmount("9")))
        assertEquals(0, BigDecimal("9.0").compareTo(parseW3sDecimalAmount("9.0")))
        assertEquals(0, BigDecimal("9.05").compareTo(parseW3sDecimalAmount("9.05")))
        assertEquals(0, BigDecimal("10000.00").compareTo(parseW3sDecimalAmount("10000.00")))
    }

    @Test
    fun `rejects more than two decimal places`() {
        assertNull(parseW3sDecimalAmount("9.005"))
    }

    @Test
    fun `rejects non numeric and malformed values`() {
        assertNull(parseW3sDecimalAmount(""))
        assertNull(parseW3sDecimalAmount("9."))
        assertNull(parseW3sDecimalAmount(".5"))
        assertNull(parseW3sDecimalAmount("9,00"))
        assertNull(parseW3sDecimalAmount("abc"))
        assertNull(parseW3sDecimalAmount("-9.00"))
    }
}
