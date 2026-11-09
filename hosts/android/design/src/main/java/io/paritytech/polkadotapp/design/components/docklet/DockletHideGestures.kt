package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.DpOffset
import kotlin.math.hypot
import kotlin.time.Duration.Companion.milliseconds

@Composable
fun rememberDockletHideSwipe(state: DockletHideState, enabled: () -> Boolean): Modifier {
    val placement = remember { DockletPlacement() }
    val currentEnabled by rememberUpdatedState(enabled)
    return Modifier
        .onGloballyPositioned { placement.coordinates = it }
        .pointerInput(state) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                if (!currentEnabled()) return@awaitEachGesture

                val downRoot = placement.rootOf(down.position)
                var direction = DockletDirection.Undecided
                var hiding = false
                if (state.isMoving) {
                    hiding = true
                    down.consume()
                    state.startHide(dockletPoint(downRoot), down.uptimeMillis.milliseconds)
                }
                var released = false
                try {
                    while (true) {
                        val change = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) {
                            released = !change.isConsumed
                            if (direction == DockletDirection.Undecided && !hiding) {
                                val upRoot = placement.rootOf(change.position)
                                val delta = upRoot - downRoot
                                direction = DockletGestures.direction(DpOffset(delta.x.toDp(), delta.y.toDp()), settling = false)
                                if (direction == DockletDirection.Sideways) {
                                    hiding = true
                                    state.startHide(dockletPoint(downRoot), down.uptimeMillis.milliseconds)
                                }
                            }
                            if (hiding) state.moveHide(dockletPoint(placement.rootOf(change.position)), change.uptimeMillis.milliseconds)
                            if (hiding || direction == DockletDirection.Vertical) change.consume()
                            break
                        }
                        val changeRoot = placement.rootOf(change.position)
                        if (direction == DockletDirection.Undecided && !hiding) {
                            val delta = changeRoot - downRoot
                            direction = DockletGestures.direction(DpOffset(delta.x.toDp(), delta.y.toDp()), settling = false)
                            if (direction == DockletDirection.Sideways) {
                                hiding = true
                                state.startHide(dockletPoint(downRoot), down.uptimeMillis.milliseconds)
                            }
                        }
                        if (hiding) state.moveHide(dockletPoint(changeRoot), change.uptimeMillis.milliseconds)
                        if (hiding || direction == DockletDirection.Vertical) change.consume()
                    }
                } finally {
                    if (hiding && !released) state.cancelDrag()
                }
                if (hiding && released) state.releaseHide()
            }
        }
}

@Composable
fun rememberDockletRestoreDrag(state: DockletHideState, onHeld: (Boolean) -> Unit): Modifier {
    val placement = remember { DockletPlacement() }
    val currentOnHeld by rememberUpdatedState(onHeld)
    return Modifier
        .onGloballyPositioned { placement.coordinates = it }
        .pointerInput(state) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                down.consume()
                currentOnHeld(true)
                val downRoot = placement.rootOf(down.position)
                var pulling = false
                var released = false
                try {
                    while (true) {
                        val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) {
                            released = !change.isConsumed
                            if (pulling) state.moveRestore(dockletPoint(placement.rootOf(change.position)), change.uptimeMillis.milliseconds)
                            change.consume()
                            break
                        }
                        val changeRoot = placement.rootOf(change.position)
                        val moved = changeRoot - downRoot
                        if (!pulling && hypot(moved.x, moved.y).toDp() >= DockletGestures.DragSlop) {
                            pulling = true
                            state.startRestore(dockletPoint(downRoot), down.uptimeMillis.milliseconds)
                        }
                        if (pulling) state.moveRestore(dockletPoint(changeRoot), change.uptimeMillis.milliseconds)
                        change.consume()
                    }
                } finally {
                    currentOnHeld(false)
                    if (pulling && !released) state.cancelDrag()
                }
                if (pulling && released) state.releaseRestore()
            }
        }
}

fun Density.dockletPoint(root: Offset) = DockletPoint(root.x.toDp().value, root.y.toDp().value)

internal class DockletPlacement {
    var coordinates: LayoutCoordinates? = null

    fun rootOf(local: Offset): Offset = coordinates?.takeIf { it.isAttached }?.localToRoot(local) ?: local
}
