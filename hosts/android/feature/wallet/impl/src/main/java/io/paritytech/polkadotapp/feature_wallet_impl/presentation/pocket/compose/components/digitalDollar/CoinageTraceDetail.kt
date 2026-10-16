package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
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
 * text gets a ground of its own.
 */
@Composable
internal fun CoinageTraceDetail(
    modifier: Modifier = Modifier,
    trace: CoinageCoinTrace
) {
    val formatter = LocalTokenAmountFormatter.current

    Column(
        modifier = modifier
            .fillMaxSize()
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
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        DetailLine(text = stringResource(RCommon.string.pocket_coinage_trace_hops, trace.hops))

        DetailLine(
            text = stringResource(RCommon.string.pocket_coinage_trace_quality, stringResource(trace.quality.label))
        )
    }
}

@Composable
private fun DetailLine(text: String) {
    NovaText(
        modifier = Modifier.height(LineHeight),
        text = text,
        maxLines = 1,
        style = PolkadotTheme.typography.body.small,
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
private val HeadlineHeight = 30.dp
private val LineHeight = 18.dp
