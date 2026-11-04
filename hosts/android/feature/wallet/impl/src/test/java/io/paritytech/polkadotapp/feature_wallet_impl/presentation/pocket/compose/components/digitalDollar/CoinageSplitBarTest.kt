package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigDecimal

class CoinageSplitBarTest {
    @Test
    fun `nothing clearing takes none of the bar`() {
        assertEquals(0f, clearingShare(BigDecimal.ZERO, BigDecimal("100")))
        assertEquals(0f, clearingShare(BigDecimal("-1"), BigDecimal("100")))
    }

    @Test
    fun `clearing takes its share of the total`() {
        assertEquals(0.25f, clearingShare(BigDecimal("25"), BigDecimal("100")))
        assertEquals(1f, clearingShare(BigDecimal("100"), BigDecimal("100")))
    }

    // A sliver would not be seen, so a little clearing still shows as a stripe.
    @Test
    fun `a little clearing still shows`() {
        assertEquals(0.06f, clearingShare(BigDecimal("1"), BigDecimal("1000")))
    }

    @Test
    fun `clearing with no total fills the bar, and never more than the bar`() {
        assertEquals(1f, clearingShare(BigDecimal("5"), BigDecimal.ZERO))
        assertEquals(1f, clearingShare(BigDecimal("150"), BigDecimal("100")))
    }
}
