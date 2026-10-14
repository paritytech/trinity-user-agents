package io.paritytech.polkadotapp.common.utils.progressStallReport

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeSource

/**
 * How long an operation may run before the user is told it is running late. Below the threshold at which people start
 * doubting a spinner, but long enough that ordinary calls never trip it.
 */
val DEFAULT_REVEAL_AFTER: Duration = 3.seconds

private val ELAPSED_TICK: Duration = 1.seconds

/**
 * Both ends of the stall report: a [StalenessReportCollector] that domain code writes regions into, and a
 * [StalenessReportDisplay] that shows them once the operation overruns [revealAfter]. The budget belongs here rather
 * than to individual steps - once any outermost region runs late, every step is shown live, running and already
 * finished alike.
 *
 * A ViewModel owns the report and the screen renders it:
 *
 * ```
 * class SendTransferViewModel(private val interactor: SendTransferInteractor) : BaseViewModel() {
 *
 *     val stalenessReport = StalenessReport(this)
 *
 *     fun confirmClicked() = launchWithDiagnostics(stalenessReport) {
 *         interactor.sendTransfer(transfer)
 *     }
 * }
 *
 * @Composable
 * fun SendTransferScreen(viewModel: SendTransferViewModel) {
 *     ...
 *     viewModel.stalenessReport.DisplayReport()
 * }
 * ```
 *
 * [DisplayReport] emits nothing while the operation is on time, so it is safe to place unconditionally.
 *
 * Concurrent branches of one operation can share a single instance and each report their own path. A finished step
 * stays on screen with its final duration - that is what tells the user the app is progressing rather than hanging -
 * and the tree is dropped once every outermost region has finished.
 */
class StalenessReport(
    private val coroutineScope: CoroutineScope,
    private val revealAfter: Duration = DEFAULT_REVEAL_AFTER,
) : StalenessReportCollector, StalenessReportDisplay {
    private val rootRegions = mutableStateListOf<TrackedRegion>()

    private var revealed by mutableStateOf(false)

    private var revealTimer: Job? = null

    // Fan-outs open their root regions from several threads at once, so arming the timer is a check-then-act that
    // has to be serialised - otherwise each of them sees an empty list and leaves a timer nothing can cancel
    @Synchronized
    override fun startRegion(region: StallableRegion): StalenessRegionHandle {
        if (rootRegions.isEmpty()) startRevealTimer()

        return TrackedRegion(region, onEnded = ::onRootRegionEnded).also(rootRegions::add)
    }

    @Composable
    override fun DisplayReport(modifier: Modifier, background: Color) {
        if (revealed) {
            RefreshElapsedTimes()

            StallReportContent(
                modifier = modifier,
                runningOperations = rootRegions.filter(TrackedRegion::isRunning).map(TrackedRegion::region).toImmutableList(),
                steps = rootRegions.flattenSteps(),
                background = background,
            )
        }
    }

    @Composable
    private fun RefreshElapsedTimes() {
        LaunchedEffect(Unit) {
            while (true) {
                rootRegions.forEach(TrackedRegion::refreshElapsed)

                delay(ELAPSED_TICK)
            }
        }
    }

    private fun startRevealTimer() {
        revealTimer?.cancel()

        revealTimer = coroutineScope.launch {
            delay(revealAfter)

            revealed = true
        }
    }

    @Synchronized
    private fun onRootRegionEnded() {
        if (rootRegions.any(TrackedRegion::isRunning)) return

        revealTimer?.cancel()
        revealed = false
        rootRegions.clear()
    }
}

/**
 * Runs [action] in a new coroutine with [diagnostics] as the collector, so everything it calls resolves its
 * `context(StalenessReportCollector)` against the report the screen is already displaying:
 *
 * ```
 * fun onApproveClicked() = launchWithDiagnostics(stalenessReport) {
 *     interactor.approveHandshake(offer).collect { ... }
 * }
 * ```
 */
fun CoroutineScope.launchWithDiagnostics(
    diagnostics: StalenessReport,
    action: suspend context(StalenessReportCollector) CoroutineScope.() -> Unit
): Job {
    return launch { action(diagnostics, this) }
}

private class TrackedRegion(
    val region: StallableRegion,
    private val onEnded: () -> Unit,
) : StalenessRegionHandle {
    val children = mutableStateListOf<TrackedRegion>()

    var isRunning by mutableStateOf(true)

    var elapsed by mutableStateOf(Duration.ZERO)

    private val startMark = TimeSource.Monotonic.markNow()

    override fun startRegion(region: StallableRegion): StalenessRegionHandle {
        return TrackedRegion(region, onEnded = {}).also(children::add)
    }

    override fun end() {
        if (!isRunning) return

        elapsed = startMark.elapsedNow()
        isRunning = false

        // A child still running here means its own `end()` was missed - close the whole subtree along with this one
        children.forEach(TrackedRegion::end)

        onEnded()
    }

    fun refreshElapsed() {
        // `end` closes the whole subtree, so a finished region can hold no running descendant worth walking into
        if (!isRunning) return

        elapsed = startMark.elapsedNow()

        children.forEach(TrackedRegion::refreshElapsed)
    }
}

private fun List<TrackedRegion>.flattenSteps(): ImmutableList<StallReportStep> {
    return buildList { appendSteps(this@flattenSteps, depth = 0) }.toImmutableList()
}

private fun MutableList<StallReportStep>.appendSteps(regions: List<TrackedRegion>, depth: Int) {
    regions.forEach { tracked ->
        add(StallReportStep(tracked.region, depth, tracked.isRunning, tracked.elapsed))

        appendSteps(tracked.children, depth + 1)
    }
}
