package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.time.Duration.Companion.nanoseconds

internal fun CoroutineScope.launchGestureMoves(
    moves: GestureMoveCoalescer<Offset>,
    position: JsGesturePosition,
    intervalMs: Int,
    onMove: (Offset) -> Unit,
): Job = launch {
    var answer = position.value.value.answer
    var lastMoveAt: Long? = null
    while (isActive && position.value.value.attached) {
        withFrameNanos { frame ->
            val received = position.value.value
            if (received.answer != answer) {
                answer = received.answer
                moves.acknowledge()
            }
            val nowMs = frame.nanoseconds.inWholeMilliseconds
            if (received.attached && GestureRules.movesNow(nowMs, lastMoveAt, intervalMs)) {
                moves.takeFrame(frame)?.let { move ->
                    lastMoveAt = nowMs
                    onMove(move)
                }
            }
        }
    }
    moves.clear()
}
