package io.paritytech.polkadotapp.common.presentation.compose

import androidx.compose.ui.text.TextRange
import org.junit.Assert.assertEquals
import org.junit.Test

private const val TICKER = "CASH"

class CurrencyTickerTest {
    @Test
    fun `should match a ticker standing on its own`() {
        assertRanges("CASH", listOf(TextRange(0, 4)))
        assertRanges("12.34 CASH", listOf(TextRange(6, 10)))
        assertRanges("Send CASH now", listOf(TextRange(5, 9)))
        assertRanges("CASH.", listOf(TextRange(0, 4)))
        assertRanges("(CASH)", listOf(TextRange(1, 5)))
    }

    @Test
    fun `should match every occurrence`() {
        assertRanges("1 CASH = 1 CASH", listOf(TextRange(2, 6), TextRange(11, 15)))
    }

    @Test
    fun `should not match a ticker inside a longer word`() {
        assertRanges("CASHIER", emptyList())
        assertRanges("1CASH", emptyList())
        assertRanges("CASH1", emptyList())
        assertRanges("xCASH", emptyList())
        assertRanges("CASHCASH", emptyList())
        assertRanges("Buy CASHBACK today", emptyList())
    }

    @Test(timeout = 5_000)
    fun `should return nothing when the ticker is absent or empty`() {
        assertRanges("no ticker here", emptyList())
        assertEquals(emptyList<TextRange>(), tickerRanges("CASH", ticker = ""))
    }

    private fun assertRanges(source: String, expected: List<TextRange>) {
        assertEquals(expected, tickerRanges(source, TICKER))
    }
}
