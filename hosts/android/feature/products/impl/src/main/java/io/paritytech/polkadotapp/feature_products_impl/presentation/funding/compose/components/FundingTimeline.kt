package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDotState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimelineUi
import kotlinx.coroutines.delay
import io.paritytech.polkadotapp.common.R as RCommon

private val MILESTONE_SIZE = 20.dp
private val MILESTONE_ICON_SIZE = 12.dp
private val DOT_SIZE = 8.dp
private val CURRENT_DOT_SIZE = 10.dp
private val LINE_HEIGHT = 2.dp
private const val LAST_HEARD_TICK_MS = 3_000L
private const val SECONDS_PER_MINUTE = 60L
private const val SECONDS_PER_HOUR = 3_600L
private const val JUST_NOW_SECONDS = 5L

/** The milestones card: the dots and their labels, then the band line under them on a darker strip. */
@Composable
fun FundingTimelineCard(
    timeline: FundingTimelineUi,
    band: String?,
    bandWarning: Boolean,
    modifier: Modifier = Modifier,
) {
    PolkadotSurface(
        modifier = modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.nested,
    ) {
        Column {
            PolkadotSurface(
                modifier = Modifier.fillMaxWidth(),
                shape = PolkadotTheme.shapes.large,
                color = PolkadotTheme.colors.bg.surface.container,
            ) {
                FundingTimelineDots(
                    modifier = Modifier.padding(
                        horizontal = PolkadotTheme.spacings.mediumIncreased,
                        vertical = PolkadotTheme.spacings.medium,
                    ),
                    timeline = timeline,
                )
            }
            if (band != null) {
                NovaText(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = PolkadotTheme.spacings.medium, vertical = PolkadotTheme.spacings.smallIncreased),
                    text = band,
                    style = PolkadotTheme.typography.body.small,
                    color = if (bandWarning) PolkadotTheme.colors.fg.warning else PolkadotTheme.colors.fg.secondary,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

@Composable
private fun FundingTimelineDots(timeline: FundingTimelineUi, modifier: Modifier = Modifier) {
    Column(modifier = modifier.fillMaxWidth()) {
        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            timeline.dots.forEachIndexed { index, state ->
                if (index > 0) {
                    Line(
                        modifier = Modifier.weight(1f),
                        color = if (state == FundingDotState.PENDING) PolkadotTheme.colors.fg.tertiary else PolkadotTheme.colors.fg.success,
                    )
                }
                if (index % FundingTimeline.MILESTONE_GAP == 0) Milestone(state) else Dot(state)
            }
        }
        Row(modifier = Modifier.fillMaxWidth().padding(top = PolkadotTheme.spacings.extraSmall)) {
            timeline.labels.forEachIndexed { index, label ->
                val dot = timeline.dots.getOrNull(index * FundingTimeline.MILESTONE_GAP)
                NovaText(
                    modifier = Modifier.weight(1f),
                    text = stringResource(label),
                    style = PolkadotTheme.typography.caption.medium,
                    color = when {
                        timeline.failedLabel == index -> PolkadotTheme.colors.fg.error
                        dot == FundingDotState.DONE -> PolkadotTheme.colors.fg.success
                        else -> PolkadotTheme.colors.fg.tertiary
                    },
                    textAlign = when (index) {
                        0 -> TextAlign.Start
                        timeline.labels.lastIndex -> TextAlign.End
                        else -> TextAlign.Center
                    },
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun Milestone(state: FundingDotState) {
    PolkadotSurface(
        modifier = Modifier.size(MILESTONE_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = state.color(),
        contentAlignment = Alignment.Center,
    ) {
        NovaIcon(
            modifier = Modifier.size(MILESTONE_ICON_SIZE),
            imageVector = NovaIcons.Check,
            tint = if (state == FundingDotState.PENDING) PolkadotTheme.colors.fg.secondary else PolkadotTheme.colors.fg.staticWhite,
        )
    }
}

@Composable
private fun Dot(state: FundingDotState) {
    val size: Dp = if (state == FundingDotState.CURRENT || state == FundingDotState.FAILED) CURRENT_DOT_SIZE else DOT_SIZE
    PolkadotSurface(modifier = Modifier.size(size), shape = PolkadotTheme.shapes.full, color = state.color()) {}
}

@Composable
private fun Line(color: Color, modifier: Modifier = Modifier) {
    Box(modifier = modifier.padding(horizontal = PolkadotTheme.spacings.extraSmall)) {
        PolkadotSurface(modifier = Modifier.fillMaxWidth().height(LINE_HEIGHT), color = color) {}
    }
}

@Composable
private fun FundingDotState.color(): Color = when (this) {
    FundingDotState.DONE -> PolkadotTheme.colors.fg.success
    FundingDotState.CURRENT -> PolkadotTheme.colors.fg.primary
    FundingDotState.FAILED -> PolkadotTheme.colors.fg.error
    FundingDotState.PENDING -> PolkadotTheme.colors.fg.tertiary
}

/** "Checked with the provider 12s ago", recounted every few seconds from [lastHeardAtMs]. */
@Composable
fun FundingLastHeard(lastHeardAtMs: Long, modifier: Modifier = Modifier) {
    val seconds by produceState(initialValue = secondsSince(lastHeardAtMs), lastHeardAtMs) {
        while (true) {
            value = secondsSince(lastHeardAtMs)
            delay(LAST_HEARD_TICK_MS)
        }
    }
    NovaText(
        modifier = modifier.fillMaxWidth(),
        text = when {
            seconds < JUST_NOW_SECONDS -> stringResource(RCommon.string.funding_last_heard_now)
            seconds < SECONDS_PER_MINUTE -> stringResource(RCommon.string.funding_last_heard_seconds, seconds.toInt())
            seconds < SECONDS_PER_HOUR -> stringResource(RCommon.string.funding_last_heard_minutes, (seconds / SECONDS_PER_MINUTE).toInt())
            else -> stringResource(RCommon.string.funding_last_heard_hours, (seconds / SECONDS_PER_HOUR).toInt())
        },
        style = PolkadotTheme.typography.caption.medium,
        color = PolkadotTheme.colors.fg.tertiary,
        textAlign = TextAlign.Center,
    )
}

private fun secondsSince(atMs: Long): Long = ((System.currentTimeMillis() - atMs) / 1_000L).coerceAtLeast(0)
