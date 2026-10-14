package io.paritytech.polkadotapp.common.utils.progressStallReport

import androidx.annotation.StringRes

/**
 * Write side of the stall report - lets a long operation declare which step it is on. Nothing reaches the user while
 * the operation keeps to time; once it overruns the budget held by the paired [StalenessReportDisplay], every step
 * declared here is shown. Steps carry no budget of their own, only a label.
 *
 * A function that can stall takes the collector as a context parameter and names its own step with [markRegion] -
 * whoever knows what the step does is who knows what to call it:
 *
 * ```
 * context(diagnostics: StalenessReportCollector)
 * suspend fun sendTransfer(transfer: Transfer): Result<Unit> = diagnostics.markRegion(RCommon.string.transfer_stall_sending) {
 *     // `estimateFee` declares a context of its own and resolves it from this body, so its region nests under this
 *     // one - no need to pass `diagnostics` down by hand
 *     val fee = feeInteractor.estimateFee(transfer)
 *
 *     submit(transfer, fee)
 * }
 * ```
 *
 * Regions form a tree: nesting comes from the callees, one region per function, and siblings may run concurrently, so
 * a single collector can serve a fan-out. Opening a second region in the same body takes [startRegion] and its handle -
 * a context parameter is not an implicit receiver, so it cannot satisfy the [markRegion] extension on its own.
 *
 * Prefer [markRegion]: it closes the region when the body throws or the coroutine is cancelled. Callers running the
 * operation with no UI attached - background sync, workers, tests - pass [NoOp].
 */
interface StalenessReportCollector {
    companion object {
        val NoOp: StalenessReportCollector = NoOpCollector
    }

    /**
     * Opens [region] and returns the handle that closes it. Regions opened on the returned handle become children of
     * [region].
     */
    fun startRegion(region: StallableRegion): StalenessRegionHandle
}

/**
 * A region that is currently open, and the collector for the regions nested inside it.
 */
interface StalenessRegionHandle : StalenessReportCollector {
    /**
     * Closes this region along with every region still open underneath it, so a child whose own [end] was missed
     * cannot outlive its parent.
     *
     * Ending an already ended region does nothing.
     */
    fun end()
}

inline fun <R> StalenessReportCollector.markRegion(region: StallableRegion, regionAction: context(StalenessReportCollector) () -> R): R {
    val handle = startRegion(region)

    return try {
        regionAction(handle)
    } finally {
        handle.end()
    }
}

inline fun <R> StalenessReportCollector.markRegion(@StringRes label: Int, regionAction: context(StalenessReportCollector) () -> R): R {
    return markRegion(ReportAsText(label), regionAction)
}

private object NoOpCollector : StalenessRegionHandle {
    override fun startRegion(region: StallableRegion): StalenessRegionHandle = this

    override fun end() {}
}
