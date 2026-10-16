package io.paritytech.polkadotapp.common.presentation.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontSynthesis
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.typography.PolkadotCashLogo
import io.paritytech.polkadotapp.designsystem.typography.PolkadotFontFamilies

enum class TickerWordmark { BySentence, Capital, SmallCaps }

@Composable
fun String.withCurrencyTickerStyle(
    style: TextStyle,
    wordmark: TickerWordmark = TickerWordmark.BySentence
): AnnotatedString = remember(this) { AnnotatedString(this) }.withCurrencyTickerStyle(style, wordmark)

@Composable
fun AnnotatedString.withCurrencyTickerStyle(
    style: TextStyle,
    wordmark: TickerWordmark = TickerWordmark.BySentence
): AnnotatedString {
    val ticker = LocalPaymentAssetBrand.current.symbol
    val spanStyle = rememberTickerSpanStyle()

    return remember(this, ticker, spanStyle, wordmark) { styleTicker(this, ticker, wordmark, spanStyle) }
}

internal fun styleTicker(
    source: AnnotatedString,
    ticker: String,
    wordmark: TickerWordmark,
    smallCaps: SpanStyle
): AnnotatedString = if (ticker == CASH_LOGO_TICKER) {
    drawTickerAsLogo(source, wordmark)
} else {
    buildAnnotatedString {
        append(source)
        styleTickerOccurrences(source.text, ticker, smallCaps)
    }
}

internal fun tickerRanges(source: String, ticker: String): List<TextRange> {
    if (ticker.isEmpty()) return emptyList()

    val ranges = mutableListOf<TextRange>()
    var start = source.indexOf(ticker)

    while (start >= 0) {
        val end = start + ticker.length
        if (source.standsAlone(start, end)) ranges += TextRange(start, end)
        start = source.indexOf(ticker, end)
    }

    return ranges
}

@Composable
private fun rememberTickerSpanStyle(): SpanStyle {
    val smallCaps = PolkadotTheme.typography.smallCaps.headlineMedium

    return remember(smallCaps) {
        // Only the face is taken: the ticker keeps the size of the text it sits in.
        SpanStyle(
            fontFamily = smallCaps.fontFamily,
            fontWeight = smallCaps.fontWeight,
            fontFeatureSettings = smallCaps.fontFeatureSettings
        )
    }
}

private fun AnnotatedString.Builder.styleTickerOccurrences(source: String, ticker: String, spanStyle: SpanStyle) {
    tickerRanges(source, ticker).forEach { range ->
        addStyle(style = spanStyle, start = range.start, end = range.end)
    }
}

// The ticker takes the glyph's case, which picks the wordmark; only its first letter is drawn, the rest stays for screen readers.
internal fun drawTickerAsLogo(source: AnnotatedString, wordmark: TickerWordmark): AnnotatedString {
    val ranges = tickerRanges(source.text, CASH_LOGO_TICKER)
    val text = ranges.fold(source.text) { text, range ->
        val glyph = source.text.logoAt(range.start, wordmark).glyph
        text.replaceRange(range.start, range.end, CASH_LOGO_TICKER.inCaseOf(glyph))
    }

    return buildAnnotatedString {
        append(AnnotatedString(text, source.spanStyles, source.paragraphStyles))

        ranges.forEach { range ->
            addStyle(CashLogoSpanStyle, range.start, range.start + 1)
            addStyle(CollapsedTickerSpanStyle, range.start + 1, range.end)
        }
    }
}

private fun String.inCaseOf(glyph: String): String = if (glyph.first().isLowerCase()) lowercase() else uppercase()

private fun String.logoAt(start: Int, wordmark: TickerWordmark): PolkadotCashLogo = when (wordmark) {
    TickerWordmark.Capital -> PolkadotCashLogo.Capital
    TickerWordmark.SmallCaps -> PolkadotCashLogo.SmallCaps
    TickerWordmark.BySentence -> if (opensSentenceAt(start)) PolkadotCashLogo.Capital else PolkadotCashLogo.SmallCaps
}

private fun String.opensSentenceAt(start: Int): Boolean =
    substring(0, start).trimEnd().lastOrNull()?.let { it in SENTENCE_ENDS } ?: true

private const val SENTENCE_ENDS = ".!?"

private const val CASH_LOGO_TICKER = "CASH"

private val CashLogoSpanStyle = SpanStyle(
    fontFamily = PolkadotFontFamilies.cashLogo,
    fontWeight = FontWeight.Normal,
    fontStyle = FontStyle.Normal,
    fontSynthesis = FontSynthesis.None
)

private val CollapsedTickerSpanStyle = SpanStyle(fontSize = 0.sp)

private fun String.standsAlone(start: Int, end: Int): Boolean =
    getOrNull(start - 1)?.isLetterOrDigit() != true && getOrNull(end)?.isLetterOrDigit() != true
