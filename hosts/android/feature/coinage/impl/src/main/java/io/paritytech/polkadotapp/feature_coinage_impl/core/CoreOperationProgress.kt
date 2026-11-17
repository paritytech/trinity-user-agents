package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import uniffi.truapi.NativeCoinage
import uniffi.truapi.NativeStatusObserver
import uniffi.truapi.Started
import uniffi.truapi.Status
import uniffi.truapi.StatusItem

/**
 * Starts the app's operation [id] with [start] and follows it until it ends, as the detection the app's claim and
 * top-up screens show. A second start under the same id joins the first.
 */
fun CoinageCore.followOperation(id: ByteArray, start: suspend (NativeCoinage) -> Started): Flow<CoinageTransferDetection> = flow {
    use(start).getOrElse { throw it }
    emitAll(followStarted(id))
}

/** Follows the app's operation [id], already started, until it ends. */
fun CoinageCore.followStarted(id: ByteArray): Flow<CoinageTransferDetection> = flow {
    val units = units().getOrElse { throw it }
    val items = observe<StatusItem> { coinage, push ->
        coinage.observeStatus(
            id,
            object : NativeStatusObserver {
                override fun onStatus(item: StatusItem) = push(item)
            }
        )
    }
    emitAll(
        items.transformWhile { item ->
            emit(item)
            !item.status.isTerminal()
        }.map { it.toDetection(units) }
    )
}

private fun Status.isTerminal() = this is Status.Done || this is Status.PartiallyDone || this is Status.Failed

private fun StatusItem.toDetection(units: CoinageUnits): CoinageTransferDetection {
    val settled = units.balanceOf(progress.settled)
    val nothingYet = progress.settled == 0UL && progress.inFlight == 0UL
    return when {
        status == Status.Done -> CoinageTransferDetection.Claimed(settled, finalized = true)
        status == Status.PartiallyDone -> CoinageTransferDetection.ClaimedPartially(settled)
        status is Status.Failed -> CoinageTransferDetection.NotClaimed
        status == Status.Settling && progress.inFlight == progress.provisional ->
            CoinageTransferDetection.Claimed(units.balanceOf(progress.settled + progress.provisional), finalized = false)
        status == Status.Waiting && nothingYet -> CoinageTransferDetection.Detecting
        else -> CoinageTransferDetection.Claiming
    }
}
