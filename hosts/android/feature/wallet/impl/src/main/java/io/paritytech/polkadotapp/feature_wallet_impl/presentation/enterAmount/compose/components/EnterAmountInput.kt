package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.TextUnit
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.fitOnOneLine
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.conditionalNotNull
import kotlin.math.roundToInt
import io.paritytech.polkadotapp.common.R as RCommon

private const val LOCKUP_MAX_WIDTH_FRACTION = 0.8f

internal data class LockupWidths(
    val fiatSymbol: Float,
    val amount: Float,
    val ticker: Float,
)

internal data class AmountSlot(
    val width: Float,
    val overflows: Boolean,
)

internal fun maxLockupWidth(boxWidth: Int): Int = (boxWidth * LOCKUP_MAX_WIDTH_FRACTION).roundToInt()

internal fun amountSlot(boxWidth: Int, widths: LockupWidths): AmountSlot {
    val limit = maxLockupWidth(boxWidth) - widths.fiatSymbol - widths.ticker
    val overflows = widths.amount > limit
    val width = if (overflows) limit.coerceAtLeast(0f) else widths.amount

    return AmountSlot(width = width, overflows = overflows)
}

@Composable
internal fun EnterAmountInput(
    modifier: Modifier = Modifier,
    input: String,
    fiatSymbol: String,
    ticker: String,
    showError: Boolean,
    enabled: Boolean = true,
    focusRequester: FocusRequester? = null,
    onInputChange: (String) -> Unit,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
            val maxStyle = PolkadotTheme.typography.display.extraLarge
            val minFontSize: TextUnit = PolkadotTheme.typography.body.medium.fontSize
            val boxWidth = constraints.maxWidth
            val density = LocalDensity.current
            val textMeasurer = rememberTextMeasurer()

            val measuredAmount = remember(input) { input.ifEmpty { "0" } }
            val tickerSuffix = remember(ticker) { " $ticker" }
            val lockup = remember(fiatSymbol, measuredAmount, tickerSuffix) { fiatSymbol + measuredAmount + tickerSuffix }
                .withCurrencyTickerStyle(maxStyle)
            val heightKeeper = remember(tickerSuffix) { "0$tickerSuffix" }.withCurrencyTickerStyle(maxStyle)

            val fitted = remember(textMeasurer, lockup, maxStyle, minFontSize, boxWidth) {
                textMeasurer.fitOnOneLine(lockup, maxStyle, minFontSize, maxLockupWidth(boxWidth))
            }
            val slot = remember(fitted, boxWidth) {
                amountSlot(boxWidth, fitted.lockupWidths(fiatSymbol.length, measuredAmount.length))
            }
            val style = remember(maxStyle, fitted) { maxStyle.copy(fontSize = fitted.layoutInput.style.fontSize) }
            val amountColor = PolkadotTheme.colors.fg.primary
            val amountStyle = remember(style, amountColor) { style.copy(color = amountColor) }
            val cursorBrush = remember(amountColor) { SolidColor(amountColor) }
            val keyboardOptions = remember { KeyboardOptions(keyboardType = KeyboardType.Decimal) }

            // This text view is needed to keep component height constant
            NovaText(
                modifier = Modifier
                    .alpha(0f)
                    .clearAndSetSemantics {},
                text = heightKeeper,
                style = maxStyle,
            )

            Row(modifier = Modifier.align(Alignment.TopCenter)) {
                NovaText(
                    modifier = Modifier.alignByBaseline(),
                    text = fiatSymbol,
                    style = style,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1
                )

                Box(
                    modifier = Modifier
                        .alignByBaseline()
                        .width(with(density) { slot.width.toDp() })
                ) {
                    BasicTextField(
                        value = input,
                        onValueChange = onInputChange,
                        singleLine = true,
                        enabled = enabled,
                        modifier = Modifier
                            .then(
                                if (slot.overflows) {
                                    Modifier.fillMaxWidth()
                                } else {
                                    Modifier.wrapContentWidth(align = Alignment.Start, unbounded = true)
                                }
                            )
                            .conditionalNotNull(focusRequester) { focusRequester(it) },
                        cursorBrush = cursorBrush,
                        keyboardOptions = keyboardOptions,
                        textStyle = amountStyle,
                        decorationBox = { inner ->
                            Box {
                                if (input.isEmpty()) {
                                    NovaText(
                                        text = measuredAmount,
                                        style = style,
                                        color = PolkadotTheme.colors.fg.tertiary
                                    )
                                }
                                inner()
                            }
                        }
                    )
                }

                NovaText(
                    modifier = Modifier.alignByBaseline(),
                    text = tickerSuffix.withCurrencyTickerStyle(style),
                    style = style,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 1,
                    softWrap = false
                )
            }
        }

        VerticalSpacer { small }

        val errorText =
            if (showError) stringResource(RCommon.string.send_enter_amount_not_enough_funds_error)
            else ""

        NovaText(
            text = errorText,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.error,
            textAlign = TextAlign.Center
        )
    }
}

private fun TextLayoutResult.lockupWidths(fiatSymbolLength: Int, amountLength: Int): LockupWidths {
    val amountStart = getHorizontalPosition(fiatSymbolLength, usePrimaryDirection = true)
    val tickerStart = getHorizontalPosition(fiatSymbolLength + amountLength, usePrimaryDirection = true)

    return LockupWidths(
        fiatSymbol = amountStart,
        amount = tickerStart - amountStart,
        ticker = getLineRight(0) - tickerStart
    )
}

@Composable
private fun EnterAmountInputPreviewContent(input: String, showError: Boolean) = CompositionLocalProvider(
    LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
) {
    PolkadotTheme {
        EnterAmountInput(
            input = input,
            fiatSymbol = "$",
            ticker = "CASH",
            showError = showError,
            enabled = true,
            onInputChange = {}
        )
    }
}

@Preview
@Preview(widthDp = 320, fontScale = 2f)
@Composable
private fun EnterAmountInputEmptyPreview() = EnterAmountInputPreviewContent(input = "", showError = true)

@Preview
@Composable
private fun EnterAmountInputSmallAmountPreview() = EnterAmountInputPreviewContent(input = "20", showError = false)

@Preview
@Composable
private fun EnterAmountInputMediumAmountPreview() = EnterAmountInputPreviewContent(input = "20000", showError = false)

@Preview
@Composable
private fun EnterAmountInputLargeAmountPreview() = EnterAmountInputPreviewContent(input = "2000000", showError = false)

@Preview
@Composable
private fun EnterAmountInputLongAmountPreview() = EnterAmountInputPreviewContent(input = "1234567890.12", showError = false)
