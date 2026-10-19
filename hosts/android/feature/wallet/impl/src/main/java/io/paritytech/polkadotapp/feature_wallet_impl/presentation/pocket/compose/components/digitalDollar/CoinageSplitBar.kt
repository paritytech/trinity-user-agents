package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.PocketMotion
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.rememberReducedMotion
import java.math.BigDecimal
import java.math.MathContext
import io.paritytech.polkadotapp.common.R as RCommon

private val BAR_HEIGHT = 6.dp
private val BAR_SHAPE = RoundedCornerShape(3.dp)
private val STRIPE_WIDTH = 4.dp
private const val MIN_CLEARING_SHARE = 0.06f

/**
 * The share of the bar the Clearing segment takes: what is clearing over the total, at least [MIN_CLEARING_SHARE]
 * so a little is still seen, and nothing when nothing is clearing.
 */
internal fun clearingShare(clearing: BigDecimal, total: BigDecimal): Float {
    if (clearing.signum() <= 0) return 0f
    if (total.signum() <= 0) return 1f
    val share = clearing.divide(total, MathContext.DECIMAL64).toFloat().coerceIn(0f, 1f)
    return share.coerceAtLeast(MIN_CLEARING_SHARE)
}

/**
 * Under the coins: a bar with the Clearing part striped, then "Clearing $X" and "Ready $Y". With nothing clearing
 * the bar is plain and "Ready" sits alone in the middle.
 */
@Composable
internal fun CoinageSplitBar(
    clearing: TokenAmountModel,
    ready: TokenAmountModel,
    total: TokenAmountModel,
    modifier: Modifier = Modifier,
) {
    val share = clearingShare(clearing.amount, total.amount)
    val track = PolkadotTheme.colors.fg.primary
    val stripe = PolkadotTheme.colors.fg.error

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(BAR_HEIGHT)
                .clip(BAR_SHAPE),
        ) {
            if (share > 0f) {
                Stripes(modifier = Modifier.weight(share).fillMaxHeight(), stripe = stripe, track = track)
            }
            if (share < 1f) {
                Canvas(modifier = Modifier.weight(1f - share).fillMaxHeight()) { drawRect(track) }
            }
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = if (share > 0f) Arrangement.SpaceBetween else Arrangement.Center,
        ) {
            if (share > 0f) SplitLabel(label = stringResource(RCommon.string.pocket_coinage_clearing), amount = clearing)
            SplitLabel(label = stringResource(RCommon.string.pocket_coinage_ready), amount = ready)
        }
    }
}

@Composable
private fun Stripes(modifier: Modifier, stripe: Color, track: Color) {
    val phase = if (rememberReducedMotion()) {
        null
    } else {
        rememberInfiniteTransition(label = "ClearingStripes").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(PocketMotion.STRIPE_PERIOD_MILLIS, easing = LinearEasing)),
            label = "ClearingStripesPhase",
        )
    }
    Canvas(modifier = modifier) {
        drawRect(track)
        val step = STRIPE_WIDTH.toPx() * 2
        var x = -size.height - step + step * (phase?.value ?: 0f)
        while (x < size.width + size.height) {
            drawLine(
                color = stripe,
                start = Offset(x, size.height),
                end = Offset(x + size.height, 0f),
                strokeWidth = STRIPE_WIDTH.toPx(),
            )
            x += step
        }
    }
}

@Composable
private fun SplitLabel(label: String, amount: TokenAmountModel) {
    val value = LocalTokenAmountFormatter.current.formatFiatSigned(amount)
    Box {
        NovaText(
            text = buildAnnotatedString {
                withStyle(SpanStyle(color = PolkadotTheme.colors.fg.secondary)) { append("$label ") }
                withStyle(SpanStyle(color = PolkadotTheme.colors.fg.primary, fontWeight = FontWeight.Medium)) { append(value) }
            },
            style = PolkadotTheme.typography.body.medium,
            maxLines = 1,
        )
    }
}
