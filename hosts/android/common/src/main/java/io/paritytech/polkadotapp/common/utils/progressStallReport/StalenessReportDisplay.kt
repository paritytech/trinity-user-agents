@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.common.utils.progressStallReport

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

interface StalenessReportDisplay {
    @Composable
    fun DisplayReport(modifier: Modifier = Modifier, background: Color = defaultStallReportBackground)
}

/**
 * Lifts the report off the surface it is placed on, which is what tells the user it is a separate notice rather than
 * more of the screen's own content. Callers on a differently coloured surface pass their own.
 */
val defaultStallReportBackground: Color
    @Composable get() = PolkadotTheme.colors.bg.surface.nested

/**
 * One row of the details sheet: what ran, how deep inside the operation it sits, and how long it has taken so far.
 */
@Immutable
data class StallReportStep(
    val region: StallableRegion,
    val depth: Int,
    val isRunning: Boolean,
    val elapsed: Duration,
)

private val GLYPH_SIZE = 14.dp
private val GLYPH_STROKE = 1.5.dp
private val STEP_INDENT = 16.dp
private const val SECONDS_PER_MINUTE = 60

/**
 * What a stalled operation looks like: a plain-language line naming the operations still in flight, and a way into
 * the full run of steps behind them.
 *
 * The split is deliberate. [runningOperations] is for someone who only needs to know the app has not frozen, so it
 * names the outermost regions and nothing else and stays small enough to sit inside whatever screen is showing it.
 * [steps] is for someone who wants to see exactly which call is hanging, and can be long, so it opens in a sheet of
 * its own rather than pushing the host screen around.
 */
@Composable
fun StallReportContent(
    modifier: Modifier = Modifier,
    runningOperations: ImmutableList<StallableRegion>,
    steps: ImmutableList<StallReportStep>,
    background: Color = defaultStallReportBackground,
) {
    var detailsShown by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .background(background, PolkadotTheme.shapes.medium)
            .padding(PolkadotTheme.spacings.small),
    ) {
        NovaText(
            text = stringResource(RCommon.string.stall_report_taking_longer),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
        )

        VerticalSpacer { small }

        // Only the outermost regions are named here - those are the units of work phrased for the user, anything
        // deeper is an implementation step and belongs in the details sheet
        Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)) {
            runningOperations.forEach { operation -> RunningOperationRow(operation) }
        }

        VerticalSpacer { small }

        DetailsAction(onClick = { detailsShown = true })
    }

    StallReportDetailsSheet(
        isVisible = detailsShown,
        steps = steps,
        onDismissRequest = { detailsShown = false },
    )
}

@Composable
private fun RunningOperationRow(operation: StallableRegion) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        StepGlyph(isRunning = true)

        HorizontalSpacer { small }

        StepLabel { operation.Label() }
    }
}

@Composable
private fun DetailsAction(onClick: () -> Unit) {
    Row(
        modifier = Modifier.clickable(onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            text = stringResource(RCommon.string.stall_report_details),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.primary,
        )

        NovaIcon(
            modifier = Modifier.size(GLYPH_SIZE),
            imageVector = NovaIcons.ArrowRight,
            tint = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Composable
private fun StallReportDetailsSheet(
    isVisible: Boolean,
    steps: ImmutableList<StallReportStep>,
    onDismissRequest: () -> Unit,
) {
    NovaModalBottomSheet(
        isVisible = isVisible,
        onDismissRequest = onDismissRequest,
    ) {
        StallReportDetails(steps)
    }
}

@Composable
private fun StallReportDetails(steps: ImmutableList<StallReportStep>) {
    Column(
        modifier = Modifier
            .padding(
                top = PolkadotTheme.spacings.large,
                bottom = PolkadotTheme.spacings.mediumIncreased,
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
            )
            // The run of steps has no natural bound - let the sheet cap its own height and scroll inside it
            .verticalScroll(rememberScrollState()),
    ) {
        NovaText(
            text = stringResource(RCommon.string.stall_report_details_title),
            style = PolkadotTheme.typography.title.medium,
            color = PolkadotTheme.colors.fg.primary,
        )

        VerticalSpacer { mediumIncreased }

        Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)) {
            steps.forEach { step -> StallReportStepRow(step) }
        }
    }
}

@Composable
private fun StallReportStepRow(step: StallReportStep) {
    Row(
        modifier = Modifier.padding(start = STEP_INDENT * step.depth),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StepGlyph(step.isRunning)

        HorizontalSpacer { small }

        StepLabel { step.region.Label() }

        HorizontalSpacer { small }

        NovaText(
            text = step.elapsed.formatElapsed(),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.tertiary,
        )
    }
}

@Composable
private fun StepGlyph(isRunning: Boolean) {
    if (isRunning) {
        NovaCircularProgressIndicator(
            modifier = Modifier.size(GLYPH_SIZE),
            strokeWidth = GLYPH_STROKE,
        )
    } else {
        NovaIcon(
            modifier = Modifier.size(GLYPH_SIZE),
            imageVector = NovaIcons.Check,
            tint = PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun StepLabel(label: @Composable () -> Unit) {
    CompositionLocalProvider(
        LocalTextStyle provides PolkadotTheme.typography.body.small,
        LocalContentColor provides PolkadotTheme.colors.fg.secondary,
        content = label,
    )
}

private fun Duration.formatElapsed(): String {
    val seconds = inWholeSeconds

    if (seconds < SECONDS_PER_MINUTE) return "${seconds}s"

    return "${seconds / SECONDS_PER_MINUTE}m ${seconds % SECONDS_PER_MINUTE}s"
}

/**
 * A stalled two-resource allocation, shared by the previews here and by the screens that host the report, so the
 * fixture only has to be kept honest in one place.
 */
fun previewStallReportOperations(): ImmutableList<StallableRegion> = persistentListOf(
    ReportAsText(RCommon.string.transaction_storage_stall_allocating),
    ReportAsText(RCommon.string.statement_store_stall_allocating_voucher),
)

fun previewStallReportSteps(): ImmutableList<StallReportStep> = persistentListOf(
    previewStep(RCommon.string.transaction_storage_stall_allocating, depth = 0, isRunning = true, seconds = 74),
    previewStep(RCommon.string.stall_reading_chain_state, depth = 1, isRunning = false, seconds = 1),
    previewStep(RCommon.string.transaction_storage_stall_picking_slot, depth = 1, isRunning = false, seconds = 2),
    previewStep(RCommon.string.stall_submitting_transaction, depth = 1, isRunning = false, seconds = 8),
    previewStep(RCommon.string.transaction_storage_stall_awaiting_bulletin, depth = 1, isRunning = true, seconds = 63),
    previewStep(RCommon.string.statement_store_stall_allocating_voucher, depth = 0, isRunning = true, seconds = 74),
    previewStep(RCommon.string.stall_reading_chain_state, depth = 1, isRunning = false, seconds = 1),
    previewStep(RCommon.string.statement_store_stall_reading_slots, depth = 1, isRunning = true, seconds = 73),
)

private fun previewStep(@StringRes label: Int, depth: Int, isRunning: Boolean, seconds: Int) = StallReportStep(
    region = ReportAsText(label),
    depth = depth,
    isRunning = isRunning,
    elapsed = seconds.seconds,
)

@Preview
@Composable
private fun StallReportContentPreview() {
    PolkadotTheme {
        StallReportContent(
            runningOperations = previewStallReportOperations(),
            steps = previewStallReportSteps(),
        )
    }
}

@Preview
@Composable
private fun StallReportDetailsPreview() {
    PolkadotTheme {
        StallReportDetails(previewStallReportSteps())
    }
}
