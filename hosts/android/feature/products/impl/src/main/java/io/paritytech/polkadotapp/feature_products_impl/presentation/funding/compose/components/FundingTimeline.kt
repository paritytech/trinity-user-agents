package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import android.content.res.Resources
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDotState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingLivePart
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimeline
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimelineUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.fundingLiveParts
import uniffi.truapi.FundingDetail
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

private val MILESTONE_SIZE = 20.dp
private val MILESTONE_ICON_SIZE = 12.dp
private val DOT_SIZE = 8.dp
private val CURRENT_DOT_SIZE = 10.dp
private val LINE_HEIGHT = 2.dp

/** The milestones card: the dots and their labels, then the band line under them on a darker strip. */
@Composable
fun FundingTimelineCard(
    modifier: Modifier = Modifier,
    timeline: FundingTimelineUi,
    band: String?,
    bandWarning: Boolean,
) {
    PolkadotSurface(
        modifier = modifier,
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
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(
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
private fun FundingTimelineDots(modifier: Modifier = Modifier, timeline: FundingTimelineUi) {
    Column(modifier = modifier) {
        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            timeline.dots.forEachIndexed { index, state ->
                if (index > 0) {
                    Line(
                        modifier = Modifier
                            .weight(1f)
                            .padding(horizontal = PolkadotTheme.spacings.extraSmall),
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
private fun Line(modifier: Modifier = Modifier, color: Color) {
    Box(modifier = modifier) {
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

/**
 * The line under the band: the provider's latest [detail] for the current step, or "Checked with the
 * provider 12s ago" from [lastHeardAt], as of [now]. One line, cut short with "…".
 */
@Composable
fun FundingLiveLine(modifier: Modifier = Modifier, detail: FundingDetail?, lastHeardAt: Instant?, now: Instant) {
    val parts = fundingLiveParts(detail, lastHeardAt, now)
    if (parts.isNotEmpty()) {
        val resources = LocalContext.current.resources
        val timeFormatter = LocalTimeFormatter.current
        NovaText(
            modifier = modifier,
            text = parts
                .map { it.resolve(resources, timeFormatter, now) }
                .reduce { line, part -> resources.getString(RCommon.string.funding_live_line_joined, line, part) },
            style = PolkadotTheme.typography.caption.medium,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

private fun FundingLivePart.resolve(resources: Resources, timeFormatter: TimeFormatter, now: Instant): String = when (this) {
    is FundingLivePart.Res ->
        resources.getString(id, *args.map { (it as? FundingLivePart)?.resolve(resources, timeFormatter, now) ?: it }.toTypedArray())
    is FundingLivePart.Verbatim -> text
    // The day alone with its weekday, else the time today or the day and time, in the device's 12/24-hour setting.
    is FundingLivePart.Moment -> when {
        dayOnly -> timeFormatter.formatDateWithWeekday(at.toEpochMilliseconds(), includeYear = false)
        else -> timeFormatter.formatDayAndTime(at.toEpochMilliseconds(), now.toEpochMilliseconds())
    }
}
