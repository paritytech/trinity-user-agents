package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.time.TimeSource

@Composable
internal fun rememberDockletCardGestures(
    state: DockletCardState,
    frames: () -> DockletCardFrames,
    pressScale: Animatable<Float, AnimationVector1D>,
    exclusion: DockletDragExclusion,
    lobe: Boolean,
    onOpenRequest: (DockletStep) -> Unit,
    onCloseRequest: () -> Unit,
    onKeyboardRequest: () -> Unit,
): Modifier {
    val latestFrames by rememberUpdatedState(frames)
    val scope = rememberCoroutineScope()
    val currentOpenRequest by rememberUpdatedState(onOpenRequest)
    val currentCloseRequest by rememberUpdatedState(onCloseRequest)
    val currentKeyboardRequest by rememberUpdatedState(onKeyboardRequest)
    val placement = remember { CardPlacement() }

    return Modifier
        .onGloballyPositioned { placement.coordinates = it }
        .pointerInput(state, exclusion, lobe) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                val atRest = !state.isOpen
                val onLobe = !atRest && lobe
                val inField = !atRest && !onLobe && exclusion.contains(placement.coordinates, down.position)
                if (inField) return@awaitEachGesture

                val ownsGesture = atRest || onLobe
                val pass = if (ownsGesture) PointerEventPass.Initial else PointerEventPass.Main
                if (ownsGesture) down.consume()
                var press = if (atRest) scope.pressIn(pressScale) else null
                fun endPress() {
                    press?.let { scope.pressOut(pressScale, it) }
                    press = null
                }
                fun rootOf(local: Offset) = placement.coordinates?.takeIf { it.isAttached }?.localToRoot(local) ?: local
                val downRoot = rootOf(down.position)
                val velocity = VelocityTracker().apply { addPosition(down.uptimeMillis, downRoot) }
                var dragging = false
                var moved = 0f
                var amount = 0f
                val travelUp = (latestFrames().rest.top - latestFrames().step2.top)
                val travelDown = (latestFrames().rest.top - state.openFrame(latestFrames()).top).coerceAtLeast(1.dp)
                var released = false

                try {
                    while (true) {
                        val change = awaitPointerEvent(pass).changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) {
                            released = DockletGestures.isRelease(change.isConsumed, ownsGesture, dragging)
                            if (ownsGesture || dragging) change.consume()
                            break
                        }
                        if (!ownsGesture && !dragging && change.previousPressed && change.isConsumed) {
                            endPress()
                            return@awaitEachGesture
                        }
                        val changeRoot = rootOf(change.position)
                        velocity.addPosition(change.uptimeMillis, changeRoot)
                        val delta = changeRoot - downRoot
                        moved = max(moved, delta.getDistance())
                        if (atRest) {
                            val upward = (-delta.y).toDp()
                            if (!dragging && DockletGestures.swipeUpStarted(DpOffset(delta.x.toDp(), delta.y.toDp()))) {
                                dragging = true
                                endPress()
                                state.startSwipeUp()
                            }
                            if (dragging) {
                                amount = DockletGestures.swipeProgress(upward, travelUp)
                                state.dragSwipe(amount)
                            }
                        } else {
                            val downward = delta.y.toDp()
                            if (!dragging && downward > DockletGestures.DragSlop) {
                                dragging = true
                                state.startSwipeDown()
                            }
                            if (dragging) {
                                amount = DockletGestures.closedFraction(downward.value, travelDown.value)
                                state.dragSwipe(1f - amount)
                            }
                        }
                        if (dragging || ownsGesture) change.consume()
                    }
                } finally {
                    if (dragging && !released) {
                        state.releaseSwipeUp(0f, 0f, travelUp, onOpened = {}, onKeyboardRequest = {})
                        state.releaseSwipeDown(0f, 0f, travelDown) {}
                    }
                }
                val speed = velocity.calculateVelocity().y.toDp().value
                endPress()
                when {
                    atRest && dragging -> state.releaseSwipeUp(
                        swipeProgress = amount,
                        upwardDpPerSecond = if (released) -speed else 0f,
                        travel = travelUp,
                        onOpened = { currentOpenRequest(DockletStep.Step2) },
                        onKeyboardRequest = { currentKeyboardRequest() },
                    )

                    dragging -> state.releaseSwipeDown(
                        closedFraction = amount,
                        downwardDpPerSecond = if (released) speed else 0f,
                        travel = travelDown,
                        onClosed = { currentCloseRequest() },
                    )

                    !released || !DockletGestures.isTap(moved.toDp()) -> Unit
                    atRest -> currentOpenRequest(DockletStep.Step1)
                    onLobe -> currentCloseRequest()
                }
            }
        }
}

@Composable
internal fun rememberDockletListDrag(
    state: DockletCardState,
    frames: () -> DockletCardFrames,
    onCloseRequest: () -> Unit,
): NestedScrollConnection {
    val latestFrames by rememberUpdatedState(frames)
    val density = LocalDensity.current
    val currentDensity by rememberUpdatedState(density)
    val currentOnClose by rememberUpdatedState(onCloseRequest)

    return remember(state) {
        object : NestedScrollConnection {
            private var dragged = 0f

            private fun travel(): Float = with(currentDensity) {
                (latestFrames().rest.top - state.openFrame(latestFrames()).top).coerceAtLeast(1.dp).toPx()
            }

            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source != NestedScrollSource.UserInput || dragged <= 0f || available.y >= 0f) return Offset.Zero

                val consumed = max(available.y, -dragged)
                dragged += consumed
                state.dragSwipe(1f - DockletGestures.closedFraction(dragged, travel()))
                return Offset(0f, consumed)
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (source != NestedScrollSource.UserInput || available.y <= 0f || !state.isOpen) return Offset.Zero

                if (dragged == 0f) state.startSwipeDown()
                dragged += available.y
                state.dragSwipe(1f - DockletGestures.closedFraction(dragged, travel()))
                return Offset(0f, available.y)
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (dragged <= 0f) return Velocity.Zero

                val fraction = DockletGestures.closedFraction(dragged, travel())
                dragged = 0f
                state.releaseSwipeDown(
                    closedFraction = fraction,
                    downwardDpPerSecond = with(currentDensity) { available.y.toDp().value },
                    travel = with(currentDensity) { travel().toDp() },
                    onClosed = { currentOnClose() },
                )
                return available
            }
        }
    }
}

private fun DockletDragExclusion.contains(coordinates: LayoutCoordinates?, local: Offset): Boolean {
    val area = bounds ?: return false
    val layout = coordinates?.takeIf { it.isAttached } ?: return false
    return area.contains(layout.localToRoot(local))
}

private class CardPlacement {
    var coordinates: LayoutCoordinates? = null
}

private class PressStart(val at: TimeSource.Monotonic.ValueTimeMark, val job: Job)

private fun CoroutineScope.pressIn(scale: Animatable<Float, AnimationVector1D>): PressStart {
    val job = launch {
        scale.animateTo(DockletMotion.PressedScale, tween(DockletMotion.PressInDuration.inWholeMilliseconds.toInt()))
    }
    return PressStart(TimeSource.Monotonic.markNow(), job)
}

private fun CoroutineScope.pressOut(scale: Animatable<Float, AnimationVector1D>, press: PressStart) {
    launch {
        val held = press.at.elapsedNow()
        if (held < DockletMotion.MinPressHold) delay(DockletMotion.MinPressHold - held)
        press.job.cancel()
        scale.animateTo(1f, tween(DockletMotion.PressOutDuration.inWholeMilliseconds.toInt()))
    }
}
