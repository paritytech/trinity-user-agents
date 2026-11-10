package io.paritytech.polkadotapp.design.components.docklet

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import androidx.compose.ui.util.lerp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.abs
import kotlin.time.Duration

enum class DockletMotionMode { Morph, CrossFade }

internal enum class DockletDrag { None, SwipeUp, SwipeDown, Settling }

@Immutable
data class DockletCardFrames(
    val rest: DockletFrame,
    val step1: DockletFrame,
    val step2: DockletFrame,
)

private val RestIconCentreBelowBody = (-1).dp
private val OpenIconCentreBelowBody = 8.dp
private val RestIconSize = 30.dp
private val OpenIconSize = 22.dp

private object UnscaledMotion : MotionDurationScale {
    override val scaleFactor = 1f
}

@Stable
class DockletCardState internal constructor(
    private val scope: CoroutineScope,
    private val haptics: DockletHaptics,
    private val currentMotionMode: () -> DockletMotionMode,
) {
    var motionMode by mutableStateOf(currentMotionMode())
        private set

    var step by mutableStateOf(DockletStep.Rest)
        private set

    var contentStep by mutableStateOf(DockletStep.Step1)
        private set

    private var drag by mutableStateOf(DockletDrag.None)

    val swipeDriven: Boolean get() = drag != DockletDrag.None

    internal val swipingUp: Boolean get() = drag == DockletDrag.SwipeUp

    private val height = Animatable(0f)
    private val width = Animatable(0f)
    private val stepBlend = Animatable(0f)
    private val crossFade = Animatable(0f)
    private var morphJob: Job? = null
    private var stepJob: Job? = null

    val isOpen: Boolean get() = step != DockletStep.Rest

    val progress: Float get() = if (motionMode == DockletMotionMode.CrossFade) crossFade.value else height.value

    val clampedProgress: Float get() = progress.coerceIn(0f, 1f)

    val stepProgress: Float get() = stepBlend.value

    val crossFadeProgress: Float get() = crossFade.value

    fun open(target: DockletStep) {
        if (isOpen || target == DockletStep.Rest) return

        motionMode = currentMotionMode()
        step = target
        contentStep = target
        drag = DockletDrag.None
        haptics.perform(DockletMoment.Open)
        morphJob?.cancel()
        morphJob = scope.launch {
            launch { moveToStep(target) }
            morph(to = 1f, duration = DockletMotion.MorphDuration, widthEasing = DockletMotion.WidthEasing)
        }
    }

    fun changeStep(target: DockletStep) {
        if (!isOpen || target == DockletStep.Rest || target == step) return

        step = target
        contentStep = target
        haptics.perform(DockletMoment.StepChange)
        stepJob?.cancel()
        stepJob = scope.launch { moveToStep(target) }
    }

    fun close() {
        if (!isOpen) return

        motionMode = currentMotionMode()
        step = DockletStep.Rest
        drag = DockletDrag.None
        haptics.perform(DockletMoment.Close)
        morphJob?.cancel()
        morphJob = scope.launch {
            morph(
                to = 0f,
                duration = DockletMotion.ContentsOutDuration + DockletMotion.MorphDuration,
                easing = DockletMotion.CloseEasing,
                widthEasing = DockletMotion.CloseEasing,
            )
        }
    }

    internal fun startSwipeUp() {
        morphJob?.cancel()
        motionMode = currentMotionMode()
        step = DockletStep.Step2
        contentStep = DockletStep.Step2
        drag = DockletDrag.SwipeUp
        haptics.perform(DockletMoment.SwipeStart)
        stepJob?.cancel()
        stepJob = scope.launch(start = CoroutineStart.UNDISPATCHED) { moveToStep(DockletStep.Step2) }
    }

    internal fun dragSwipe(openAmount: Float) {
        if (drag != DockletDrag.SwipeUp && drag != DockletDrag.SwipeDown) return

        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            height.snapTo(openAmount)
            width.snapTo(DockletMotion.widthProgress(openAmount))
            crossFade.snapTo(openAmount.coerceIn(0f, 1f))
        }
    }

    internal fun releaseSwipeUp(
        swipeProgress: Float,
        upwardDpPerSecond: Float,
        travel: Dp,
        onOpened: () -> Unit,
        onKeyboardRequest: () -> Unit,
    ) {
        if (drag != DockletDrag.SwipeUp) return

        drag = DockletDrag.Settling
        val release = DockletGestures.releaseSwipeUp(swipeProgress, upwardDpPerSecond, travel)
        val opens = release == DockletRelease.Open
        val remaining = travel * if (opens) 1f - swipeProgress else swipeProgress
        val duration = DockletGestures.remainingMotion(remaining, abs(upwardDpPerSecond))

        if (opens) {
            haptics.perform(DockletMoment.Open)
            onOpened()
        } else {
            step = DockletStep.Rest
        }
        morphJob?.cancel()
        morphJob = scope.launch {
            if (opens) {
                launch {
                    delay(DockletGestures.keyboardRequestAt(duration))
                    onKeyboardRequest()
                }
            }
            morph(to = if (opens) 1f else 0f, duration = duration)
            drag = DockletDrag.None
        }
    }

    internal fun startSwipeDown() {
        if (!isOpen) return

        morphJob?.cancel()
        drag = DockletDrag.SwipeDown
    }

    internal fun releaseSwipeDown(closedFraction: Float, downwardDpPerSecond: Float, travel: Dp, onClosed: () -> Unit) {
        if (!isOpen || drag != DockletDrag.SwipeDown) return

        drag = DockletDrag.Settling
        val release = DockletGestures.releaseSwipeDown(closedFraction, downwardDpPerSecond, step)
        val closes = release == DockletRelease.Close
        val remaining = travel * if (closes) 1f - closedFraction else closedFraction
        val duration = DockletGestures.remainingMotion(remaining, abs(downwardDpPerSecond))

        if (closes) {
            step = DockletStep.Rest
            haptics.perform(DockletMoment.Close)
            onClosed()
        }
        morphJob?.cancel()
        morphJob = scope.launch {
            morph(to = if (closes) 0f else 1f, duration = duration)
            drag = DockletDrag.None
        }
    }

    fun openFrame(frames: DockletCardFrames): DockletFrame = lerpFrame(frames.step1, frames.step2, stepBlend.value)

    fun frame(frames: DockletCardFrames): DockletFrame {
        val open = openFrame(frames)
        val heightAmount = if (motionMode == DockletMotionMode.CrossFade) 1f else height.value
        val widthAmount = if (motionMode == DockletMotionMode.CrossFade) 1f else width.value
        val frameWidth = lerp(frames.rest.width, open.width, widthAmount)
        val frameHeight = lerp(frames.rest.height, open.height, heightAmount)
        val bottom = lerp(frames.rest.bottom, open.bottom, heightAmount)
        val centerX = lerp(frames.rest.centerX, open.centerX, widthAmount)
        return DockletFrame(centerX - frameWidth / 2, bottom - frameHeight, frameWidth, frameHeight)
    }

    private suspend fun moveToStep(target: DockletStep) {
        val blend = if (target == DockletStep.Step2) 1f else 0f
        if (height.value <= 0f) {
            stepBlend.snapTo(blend)
        } else {
            stepBlend.animateTo(blend, DockletMotion.stepChangeSpec())
        }
    }

    private suspend fun morph(
        to: Float,
        duration: Duration,
        easing: Easing = DockletMotion.HeightEasing,
        widthEasing: Easing = DockletMotion.HeightEasing,
    ) {
        val millis = duration.inWholeMilliseconds.toInt()
        if (motionMode == DockletMotionMode.CrossFade) {
            val crossFadeMillis = DockletMotion.CrossFadeDuration.inWholeMilliseconds.toInt()
            withContext(UnscaledMotion) {
                crossFade.animateTo(to, tween(crossFadeMillis, easing = LinearEasing))
            }
            height.snapTo(to)
            width.snapTo(to)
        } else {
            coroutineScope {
                launch { height.animateTo(to, tween(millis, easing = easing)) }
                launch { width.animateTo(to, tween(millis, easing = widthEasing)) }
            }
            crossFade.snapTo(to)
        }
    }
}

@Composable
fun rememberDockletCardState(): DockletCardState {
    val scope = rememberCoroutineScope()
    val haptics = rememberDockletHaptics()
    return remember(scope, haptics) {
        DockletCardState(scope, haptics, ::systemMotionMode)
    }
}

private fun systemMotionMode() =
    if (ValueAnimator.areAnimatorsEnabled()) DockletMotionMode.Morph else DockletMotionMode.CrossFade

internal fun dockletIconSize(progress: Float): Dp = lerp(RestIconSize, OpenIconSize, progress)

internal fun dockletIconCentreBelowBody(progress: Float): Dp =
    lerp(RestIconCentreBelowBody, OpenIconCentreBelowBody, progress)

private fun lerpFrame(start: DockletFrame, stop: DockletFrame, fraction: Float) = DockletFrame(
    left = lerp(start.left, stop.left, fraction),
    top = lerp(start.top, stop.top, fraction),
    width = lerp(start.width, stop.width, fraction),
    height = lerp(start.height, stop.height, fraction),
)
