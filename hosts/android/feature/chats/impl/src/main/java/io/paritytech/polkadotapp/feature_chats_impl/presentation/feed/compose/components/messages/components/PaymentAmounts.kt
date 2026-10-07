package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextDecoration
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.fitOnOneLine
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

@Composable
internal fun PaymentAmounts(
    amount: TokenAmountModel,
    differingAmount: TokenAmountModel?,
    maxWidthPx: Int,
    primaryTextColor: Color,
    secondaryTextColor: Color
) {
    val formatter = LocalTokenAmountFormatter.current

    Column(horizontalAlignment = Alignment.Start) {
        if (differingAmount != null) {
            val struckStyle = PolkadotTheme.typography.body.medium.copy(textDecoration = TextDecoration.LineThrough)

            NovaText(
                text = formatter.formatFiatSigned(amount, withSymbol = true).withCurrencyTickerStyle(struckStyle),
                style = struckStyle,
                color = secondaryTextColor
            )
        }

        PaymentAmountLine(
            amount = differingAmount ?: amount,
            maxWidthPx = maxWidthPx,
            primaryTextColor = primaryTextColor,
            tickerColor = secondaryTextColor
        )
    }
}

@Composable
private fun PaymentAmountLine(
    amount: TokenAmountModel,
    maxWidthPx: Int,
    primaryTextColor: Color,
    tickerColor: Color
) {
    val style = PolkadotTheme.typography.headline.large
    val minFontSize = PolkadotTheme.typography.body.medium.fontSize
    val formatter = LocalTokenAmountFormatter.current
    val text = formatter.formatFiatSigned(amount, withSymbol = true)
    val ticker = formatter.formatToSymbol(amount)
    val lockup = remember(text, ticker, tickerColor) {
        buildAnnotatedString {
            append(text)
            addStyle(SpanStyle(color = tickerColor), start = text.length - ticker.length, end = text.length)
        }
    }.withCurrencyTickerStyle(style)
    val textMeasurer = rememberTextMeasurer(cacheSize = 0)
    val fittedStyle = remember(textMeasurer, lockup, style, minFontSize, maxWidthPx) {
        val fitted = textMeasurer.fitOnOneLine(lockup, style, minFontSize, maxWidthPx)
        style.copy(fontSize = fitted.layoutInput.style.fontSize)
    }

    NovaText(
        text = lockup,
        style = fittedStyle,
        color = primaryTextColor,
        maxLines = 1,
        softWrap = false
    )
}
