package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.icon.vectors.InfoFilled
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageArrangement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageCoinTrace
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.HoldingGeometry
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * What a picked-out coin says about itself, beside it in the band. It floats over the faded coins, so the
 * text gets a ground of its own, sized to its content: opening the quality explanation only ever grows it
 * downward. The band is already placed for it at its tallest, so it never has to move or reach off screen.
 *
 * The explanation is inline rather than a popup: the band is itself a floating panel. Its open state lives
 * with this pick, so picking another coin, putting the coin back or collapsing the grid closes it.
 */
@Composable
internal fun CoinageTraceDetail(
    modifier: Modifier = Modifier,
    trace: CoinageCoinTrace
) {
    val formatter = LocalTokenAmountFormatter.current
    var explained by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(
                color = PolkadotTheme.colors.bg.surface.container.copy(alpha = GROUND_ALPHA),
                shape = RoundedCornerShape(HoldingGeometry.containerCorner)
            )
            .padding(horizontal = PolkadotTheme.spacings.small, vertical = CoinageArrangement.FOCUS_INSET.dp)
    ) {
        Row(
            modifier = Modifier.height(HeadlineHeight),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall),
            verticalAlignment = Alignment.CenterVertically
        ) {
            NovaText(
                modifier = Modifier.alignByBaseline(),
                text = formatter.formatFiatSigned(trace.value),
                maxLines = 1,
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary
            )

            NovaText(
                modifier = Modifier.alignByBaseline(),
                text = stringResource(trace.partition.label),
                maxLines = 1,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        DetailLine(
            modifier = Modifier.height(LineHeight),
            text = stringResource(RCommon.string.pocket_coinage_trace_prior_owners, trace.hops)
        )

        Row(modifier = Modifier.heightIn(min = LineHeight), verticalAlignment = Alignment.CenterVertically) {
            // Not pinned to the line height, so the row centres the text and the mark on one axis.
            DetailLine(
                text = stringResource(RCommon.string.pocket_coinage_trace_quality, stringResource(trace.quality.label))
            )

            HorizontalSpacer { tiny }

            PolkadotIconButton(
                icon = if (explained) NovaIcons.InfoFilled else NovaIcons.Info,
                onClick = { explained = !explained },
                style = PolkadotButtonStyle.ghost(),
                size = PolkadotIconButtonSize.tiny()
            )
        }

        AnimatedVisibility(
            visible = explained,
            enter = fadeIn(tween(FADE_MILLIS)),
            exit = fadeOut(tween(FADE_MILLIS))
        ) {
            QualityExplanation()
        }
    }
}

@Composable
private fun QualityExplanation() {
    val plain = stringResource(RCommon.string.pocket_coinage_trace_quality_explanation)
    val aside = stringResource(RCommon.string.pocket_coinage_trace_quality_explanation_aside)

    NovaText(
        modifier = Modifier.padding(
            start = PolkadotTheme.spacings.medium,
            top = PolkadotTheme.spacings.extraSmall,
            bottom = PolkadotTheme.spacings.small
        ),
        text = buildAnnotatedString {
            append(plain)
            append(" ")
            withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(aside) }
        },
        style = PolkadotTheme.typography.body.small,
        color = PolkadotTheme.colors.fg.secondary
    )
}

@Composable
private fun DetailLine(
    modifier: Modifier = Modifier,
    text: String
) {
    NovaText(
        modifier = modifier,
        text = text,
        maxLines = 1,
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.secondary
    )
}

private val CoinageCoinTrace.Quality.label: Int
    get() = when (this) {
        CoinageCoinTrace.Quality.UNKNOWN -> RCommon.string.pocket_coinage_trace_quality_unknown
        CoinageCoinTrace.Quality.VERY_LOW -> RCommon.string.pocket_coinage_trace_quality_very_low
        CoinageCoinTrace.Quality.LOW -> RCommon.string.pocket_coinage_trace_quality_low
        CoinageCoinTrace.Quality.FAIR -> RCommon.string.pocket_coinage_trace_quality_fair
        CoinageCoinTrace.Quality.GOOD -> RCommon.string.pocket_coinage_trace_quality_good
        CoinageCoinTrace.Quality.HIGH -> RCommon.string.pocket_coinage_trace_quality_high
    }

private const val GROUND_ALPHA = 0.92f
private const val FADE_MILLIS = 200
private val HeadlineHeight = 30.dp
private val LineHeight = 24.dp
