package io.paritytech.polkadotapp.common.presentation.compose

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontSynthesis
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.designsystem.typography.PolkadotFontFamilies
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val TICKER = "CASH"

private val SMALL_CAPS = SpanStyle(fontFeatureSettings = "c2sc")

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

    @Test
    fun `should draw a ticker inside a sentence as the small caps wordmark`() {
        val logoText = drawTickerAsLogo(AnnotatedString("12.34 CASH"), wordmark = TickerWordmark.BySentence)

        assertEquals("12.34 cash", logoText.text)
        assertEquals(listOf(TextRange(6, 7)), logoText.logoRanges())
        assertEquals(listOf(TextRange(7, 10)), logoText.collapsedRanges())
    }

    @Test
    fun `should draw a ticker that opens the text as the capital wordmark`() {
        val logoText = drawTickerAsLogo(AnnotatedString("CASH Privacy Mode"), wordmark = TickerWordmark.BySentence)

        assertEquals("CASH Privacy Mode", logoText.text)
        assertEquals(listOf(TextRange(0, 1)), logoText.logoRanges())
        assertEquals(listOf(TextRange(1, 4)), logoText.collapsedRanges())
    }

    @Test
    fun `should draw a ticker after a sentence end as the capital wordmark`() {
        assertLogo(source = "Done. CASH arrives soon", text = "Done. CASH arrives soon")
        assertLogo(source = "Done! CASH", text = "Done! CASH")
        assertLogo(source = "Done? CASH", text = "Done? CASH")
        assertLogo(source = "Step 2. CASH is ready", text = "Step 2. CASH is ready")
    }

    @Test
    fun `should pick the wordmark for each ticker on its own`() {
        assertLogo(source = "CASH sent, 5 CASH left", text = "CASH sent, 5 cash left", logo = listOf(TextRange(0, 1), TextRange(13, 14)))
        assertLogo(source = "1 CASH = 1 CASH", text = "1 cash = 1 cash", logo = listOf(TextRange(2, 3), TextRange(11, 12)))
        assertLogo(source = "Add CASH via bank", text = "Add cash via bank")
        assertLogo(source = " CASH", text = " CASH")
    }

    @Test
    fun `should draw a ticker that continues an amount as the small caps wordmark`() {
        assertEquals("cash", drawTickerAsLogo(AnnotatedString("CASH"), wordmark = TickerWordmark.SmallCaps).text)
        assertEquals(" cash", drawTickerAsLogo(AnnotatedString(" CASH"), wordmark = TickerWordmark.SmallCaps).text)
        assertEquals("Send 12. cash", drawTickerAsLogo(AnnotatedString("Send 12. CASH"), wordmark = TickerWordmark.SmallCaps).text)
    }

    @Test
    fun `should draw a ticker inside a sentence as the capital wordmark when asked`() {
        val logoText = drawTickerAsLogo(AnnotatedString("Add CASH via bank"), wordmark = TickerWordmark.Capital)

        assertEquals("Add CASH via bank", logoText.text)
        assertEquals(listOf(TextRange(4, 5)), logoText.logoRanges())
    }

    @Test
    fun `should leave a ticker inside a longer word untouched`() {
        assertLogo(source = "Buy CASHBACK today", text = "Buy CASHBACK today", logo = emptyList())
    }

    @Test
    fun `should keep the styles the ticker already had on the logo glyph`() {
        val secondary = SpanStyle(color = Color.Gray)
        val source = buildAnnotatedString {
            append("$20 ")
            withStyle(secondary) { append("CASH") }
        }

        val logoText = drawTickerAsLogo(source, wordmark = TickerWordmark.BySentence)

        assertEquals(listOf(TextRange(4, 8)), logoText.spanStyles.filter { it.item == secondary }.map { TextRange(it.start, it.end) })
    }

    @Test
    fun `should draw the logo at its regular weight inside bold text`() {
        val source = buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("2 CASH") }
        }

        val styles = drawTickerAsLogo(source, wordmark = TickerWordmark.BySentence).spanStyles
        val logo = styles.single { it.item.fontFamily == PolkadotFontFamilies.cashLogo }

        assertEquals(FontWeight.Normal, logo.item.fontWeight)
        assertEquals(FontSynthesis.None, logo.item.fontSynthesis)
        assertTrue(styles.indexOf(logo) > styles.indexOfFirst { it.item.fontWeight == FontWeight.Bold && it.start <= logo.start && it.end > logo.start })
    }

    @Test
    fun `should draw the wordmark only for the CASH brand`() {
        val logoText = styleTicker(AnnotatedString("12.34 CASH"), TICKER, TickerWordmark.BySentence, SMALL_CAPS)

        assertEquals("12.34 cash", logoText.text)
        assertEquals(listOf(TextRange(6, 7)), logoText.logoRanges())
        assertTrue(logoText.spanStyles.none { it.item == SMALL_CAPS })
    }

    @Test
    fun `should keep the small caps ticker for any other brand`() {
        val tickerText = styleTicker(AnnotatedString("12.34 USDX"), "USDX", TickerWordmark.BySentence, SMALL_CAPS)

        assertEquals("12.34 USDX", tickerText.text)
        assertEquals(emptyList<TextRange>(), tickerText.logoRanges())
        assertEquals(listOf(TextRange(6, 10)), tickerText.spanStyles.filter { it.item == SMALL_CAPS }.map { TextRange(it.start, it.end) })
    }

    @Test
    fun `should keep a paragraph style that spans the ticker in one piece`() {
        val centered = ParagraphStyle(textAlign = TextAlign.Center)
        val source = buildAnnotatedString {
            withStyle(centered) { append("Pay 5 CASH now") }
        }

        val logoText = drawTickerAsLogo(source, wordmark = TickerWordmark.BySentence)

        assertEquals(listOf(TextRange(0, 14)), logoText.paragraphStyles.map { TextRange(it.start, it.end) })
    }

    private fun assertRanges(source: String, expected: List<TextRange>) {
        assertEquals(expected, tickerRanges(source, TICKER))
    }

    private fun assertLogo(source: String, text: String, logo: List<TextRange>? = null) {
        val logoText = drawTickerAsLogo(AnnotatedString(source), wordmark = TickerWordmark.BySentence)

        assertEquals(text, logoText.text)
        logo?.let { assertEquals(it, logoText.logoRanges()) }
    }

    private fun AnnotatedString.logoRanges() = spanStyles
        .filter { it.item.fontFamily == PolkadotFontFamilies.cashLogo }
        .map { TextRange(it.start, it.end) }

    private fun AnnotatedString.collapsedRanges() = spanStyles
        .filter { it.item.fontSize == 0.sp }
        .map { TextRange(it.start, it.end) }
}
