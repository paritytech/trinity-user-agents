package io.paritytech.polkadotapp.feature_connection_status_api.presentation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainLiveness
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import kotlinx.collections.immutable.toImmutableList
import kotlin.math.roundToInt
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.DurationUnit
import io.paritytech.polkadotapp.common.R as RCommon

private const val PERCENT = 100

/**
 * The "Network Status" breakdown behind the tab bar's connectivity item. A connected, producing chain also
 * shows the share of expected blocks it produced and the interval that implies; the statement store,
 * which produces none, shows neither.
 */
@Composable
fun ChainHealthPanel(
    modifier: Modifier = Modifier,
    model: ChainHealthIndicatorsModel,
) {
    Column(modifier = modifier) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large),
        ) {
            NovaText(
                text = stringResource(RCommon.string.chain_health_panel_title),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
            )
            model.rows.forEach { item -> HealthRow(item = item) }
        }
    }
}

@Composable
private fun HealthRow(item: ChainHealthItemModel) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RowIndicator(modifier = Modifier.clearAndSetSemantics { }, item = item, indicatorSize = ChainIndicatorSize.Panel)
        Column {
            NovaText(
                text = stringResource(item.row.nameRes()),
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary,
            )
            NovaText(
                text = summary(item),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun summary(item: ChainHealthItemModel): String {
    val state = stringResource(item.labelRes())

    return when (val indicator = item.indicator) {
        is ChainHealthIndicator.Healthy -> connectedSummary(item.row, state, indicator.liveness)
        is ChainHealthIndicator.Production -> connectedSummary(item.row, state, indicator.liveness)
        ChainHealthIndicator.Outage,
        ChainHealthIndicator.Connecting,
        ChainHealthIndicator.Disconnected,
        ChainHealthIndicator.Offline,
        -> state
    }
}

@Composable
private fun connectedSummary(row: IndicatorRow, state: String, liveness: ChainLiveness?): String =
    if (row.measuresProduction()) productionSummary(state, liveness) else state

@Composable
private fun productionSummary(state: String, liveness: ChainLiveness?): String = stringResource(
    RCommon.string.chain_health_connected_summary,
    state,
    liveness?.let { stringResource(RCommon.string.chain_health_live_percent, (it.share * PERCENT).roundToInt()) }
        ?: stringResource(RCommon.string.chain_health_value_unknown),
    liveness?.let { stringResource(RCommon.string.chain_health_block_interval, duration(it.blockInterval)) }
        ?: stringResource(RCommon.string.chain_health_value_unknown),
)

@Composable
private fun duration(value: Duration): String {
    val seconds = value.toDouble(DurationUnit.SECONDS).roundToInt()

    return if (seconds < 1.minutes.inWholeSeconds) {
        stringResource(RCommon.string.chain_health_duration_seconds, seconds)
    } else {
        stringResource(
            RCommon.string.chain_health_duration_minutes,
            seconds / 1.minutes.inWholeSeconds,
            seconds % 1.minutes.inWholeSeconds,
        )
    }
}

// The design names a row by its role, not by the registry name of whichever network is active.
internal fun IndicatorRow.nameRes(): Int = when (this) {
    IndicatorRow.People -> RCommon.string.chain_health_chain_people
    IndicatorRow.AssetHub -> RCommon.string.chain_health_chain_hub
    IndicatorRow.Bulletin -> RCommon.string.chain_health_chain_bulletin
    IndicatorRow.StatementStore -> RCommon.string.chain_health_statement_store
}

internal fun IndicatorRow.measuresProduction(): Boolean = when (this) {
    IndicatorRow.People, IndicatorRow.AssetHub, IndicatorRow.Bulletin -> true
    IndicatorRow.StatementStore -> false
}

internal fun ChainHealthItemModel.labelRes(): Int = when (indicator) {
    is ChainHealthIndicator.Healthy, is ChainHealthIndicator.Production -> RCommon.string.chain_health_state_connected
    ChainHealthIndicator.Outage -> RCommon.string.chain_health_state_not_producing
    ChainHealthIndicator.Connecting -> RCommon.string.chain_health_state_connecting
    ChainHealthIndicator.Disconnected -> row.brokenLabelRes()
    ChainHealthIndicator.Offline -> RCommon.string.chain_health_state_offline
}

private fun IndicatorRow.brokenLabelRes(): Int = when (this) {
    IndicatorRow.People, IndicatorRow.AssetHub, IndicatorRow.Bulletin -> RCommon.string.chain_health_state_broken
    IndicatorRow.StatementStore -> RCommon.string.chain_health_state_disconnected
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChainHealthPanelHealthyPreview() {
    PanelPreview(
        previewItem(IndicatorRow.People, share = 13f / 15f, blockTime = 2.seconds),
        previewItem(IndicatorRow.AssetHub, share = 1f, blockTime = 2.seconds),
        previewItem(IndicatorRow.Bulletin, share = 0.9f, blockTime = 6.seconds),
        previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Healthy(liveness = null)),
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChainHealthPanelMixedPreview() {
    PanelPreview(
        previewItem(IndicatorRow.People, share = 0.8f, blockTime = 2.seconds),
        previewItem(IndicatorRow.AssetHub, share = 0.4f, blockTime = 2.seconds),
        previewItem(IndicatorRow.Bulletin, share = 0f, blockTime = 6.seconds),
        previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Connecting),
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChainHealthPanelDisconnectedPreview() {
    PanelPreview(
        previewItem(IndicatorRow.People, ChainHealthIndicator.Disconnected),
        previewItem(IndicatorRow.AssetHub, ChainHealthIndicator.Offline),
        previewItem(IndicatorRow.Bulletin, ChainHealthIndicator.Connecting),
        previewItem(IndicatorRow.StatementStore, ChainHealthIndicator.Disconnected),
    )
}

@Composable
private fun PanelPreview(vararg rows: ChainHealthItemModel) {
    PolkadotTheme {
        ChainHealthPanel(
            modifier = Modifier.background(PolkadotTheme.colors.bg.surface.container),
            model = ChainHealthIndicatorsModel(rows.toList().toImmutableList()),
        )
    }
}

private fun previewItem(row: IndicatorRow, share: Float, blockTime: Duration) =
    previewItem(row, ChainHealthIndicator.of(share, blockTime))

private fun previewItem(row: IndicatorRow, indicator: ChainHealthIndicator) = ChainHealthItemModel(
    row = row,
    indicator = indicator,
)
