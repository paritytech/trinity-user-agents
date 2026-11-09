package io.paritytech.polkadotapp.design.components.docklet

import android.animation.ValueAnimator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.max
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

enum class DockletLanding { Shown, HiddenBySwipe, HiddenQuietly }

@Stable
class DockletHideState internal constructor(
    private val scope: CoroutineScope,
    private val haptics: DockletHaptics,
    private val animationsEnabled: () -> Boolean,
    private val onLanded: DockletHideState.(DockletLanding) -> Unit,
) {
    private val progress = Animatable(0f, DockletSpring.Threshold)
    private val residual = Animatable(0f)
    private val peek = Animatable(0f)
    private val barAlpha = Animatable(1f)

    var drawbridge: DockletDrawbridge? by mutableStateOf(null)

    private var hold: DockletHold? by mutableStateOf(null)
    private var aim: DockletAim? by mutableStateOf(null)
    private var dragProgress by mutableFloatStateOf(0f)
    private var target by mutableFloatStateOf(0f)
    private var landingPending = false
    private var quiet by mutableStateOf(false)
    private var settling by mutableStateOf(false)
    private var residualPose by mutableStateOf(DockletPose.Zero)
    private var tracker: DockletReleaseTracker? = null
    private var travel = 1f
    private var zeroX = 0f
    private var pastHalf = false
    private var settleJob: Job? = null
    private var peekJob: Job? = null

    val countsAsHidden: Boolean get() = target == 1f

    val isShownAtRest: Boolean get() = target == 0f && !isMoving && progress.value == 0f

    val isMoving: Boolean get() = hold != null || aim != null || settling

    val isHeld: Boolean get() = hold != null || aim != null

    val isOffScreen: Boolean get() = target == 1f && !isMoving && peek.value == 0f

    val alpha: Float get() = barAlpha.value

    val glowAlpha: Float get() = max(currentProgress(), 1f - barAlpha.value)

    fun quietFade(): Float = if (quiet) currentProgress() else 0f

    fun pose(): DockletPose {
        val bridge = drawbridge ?: return DockletPose.Zero
        hold?.let { return it.pose() }
        aim?.let { return it.pose() }
        return bridge.plainPose(screenProgress()).plus(residualPose, residual.value)
    }

    fun startHide(touch: DockletPoint, time: Duration) {
        val bridge = drawbridge ?: return
        val screenPose = pose()
        val start = stopMotion()
        travel = bridge.hideTravel(touch.x)
        zeroX = touch.x - start * travel
        hold = DockletHold(bridge, touch, start, travel, screenPose)
        dragProgress = start
        pastHalf = start >= HalfWay
        tracker = DockletReleaseTracker(time, start, touch)
    }

    fun moveHide(finger: DockletPoint, time: Duration) {
        val held = hold ?: return
        val samples = tracker ?: return
        dragProgress = DockletDrawbridge.progressFromTravel((finger.x - zeroX) / travel)
        held.track(finger, dragProgress)
        samples.add(time, dragProgress, finger)
        val nowPast = DockletSpring.pastHalf(pastHalf, DockletSpring.projected(dragProgress, samples.velocity(travel)))
        if (nowPast != pastHalf) haptics.perform(DockletMoment.HideCross)
        pastHalf = nowPast
    }

    fun releaseHide() {
        val held = hold ?: return
        val samples = tracker ?: return
        val start = samples.startProgress
        val velocity = samples.velocity(travel)
        val goal = DockletSpring.hideTarget(start, dragProgress, velocity, samples.turnedBack())
        residualPose = held.pose().minus(drawbridge?.plainPose(dragProgress) ?: DockletPose.Zero)
        hold = null
        tracker = null
        quiet = false
        settle(from = dragProgress, to = goal, velocity = velocity, spec = DockletSpring.afterRelease(velocity))
    }

    fun startRestore(touch: DockletPoint, time: Duration) {
        val bridge = drawbridge ?: return
        val screenPose = pose()
        val start = stopMotion()
        aim = DockletAim(bridge, touch, start, screenPose)
        dragProgress = start
        travel = bridge.hideTravel(touch.x)
        tracker = DockletReleaseTracker(time, start, touch)
    }

    fun moveRestore(finger: DockletPoint, time: Duration) {
        val pulling = aim ?: return
        pulling.track(finger)
        dragProgress = pulling.progress
        tracker?.add(time, dragProgress, finger)
    }

    fun releaseRestore() {
        val pulling = aim ?: return
        val samples = tracker ?: return
        val velocity = samples.velocity(travel)
        val goal = DockletSpring.restoreTarget(dragProgress, velocity, samples.fingerSpeed(), samples.turnedBack())
        residualPose = pulling.pose().minus(drawbridge?.plainPose(dragProgress) ?: DockletPose.Zero)
        aim = null
        tracker = null
        settle(from = dragProgress, to = goal, velocity = velocity, spec = DockletSpring.afterRelease(velocity))
    }

    fun cancelDrag() {
        val wasHeld = hold != null || aim != null
        if (!wasHeld) return
        val goal = if (dragProgress >= HalfWay) 1f else 0f
        residualPose = (hold?.pose() ?: aim?.pose() ?: DockletPose.Zero).minus(drawbridge?.plainPose(dragProgress) ?: DockletPose.Zero)
        hold = null
        aim = null
        tracker = null
        settle(from = dragProgress, to = goal, velocity = 0f, spec = DockletSpring.Move)
    }

    fun quietHide() {
        if (!isShownAtRest || drawbridge == null) return
        quiet = true
        landingPending = true
        target = 1f
        settling = true
        settleJob = scope.launch {
            withFrameNanos { }
            withFrameNanos { }
            runSettle(from = 0f, to = 1f, velocity = 0f, spec = DockletSpring.Quiet)
        }
    }

    fun tuck() = snapTo(1f)

    fun untuck() = snapTo(0f)

    private fun snapTo(to: Float) {
        if (isHeld) return
        stopMotion()
        landingPending = false
        quiet = false
        target = to
        settleJob = scope.launch(start = CoroutineStart.UNDISPATCHED) {
            peek.snapTo(0f)
            progress.snapTo(to)
            barAlpha.snapTo(1f)
        }
    }

    fun restore() {
        if (target == 0f && !isHeld) return
        val start = stopMotion()
        residualPose = DockletPose.Zero
        settle(from = start, to = 0f, velocity = 0f, spec = DockletSpring.Move)
    }

    fun peek() {
        peekJob?.cancel()
        peekJob = scope.launch {
            delay(PeekDelay)
            peek.animateTo(1f, tween(PeekMillis, easing = LinearEasing))
            peek.snapTo(0f)
        }
    }

    private fun settle(from: Float, to: Float, velocity: Float, spec: SpringSpec<Float>) {
        if (to != target) landingPending = true
        target = to
        settleJob?.cancel()
        settling = true
        settleJob = scope.launch { runSettle(from, to, velocity, spec) }
    }

    private suspend fun runSettle(from: Float, to: Float, velocity: Float, spec: SpringSpec<Float>) {
        peek.snapTo(0f)
        if (animationsEnabled()) {
            progress.snapTo(from)
            coroutineScope {
                launch {
                    residual.snapTo(1f)
                    residual.animateTo(0f, DockletSpring.Move)
                }
                progress.animateTo(to, spec, initialVelocity = velocity)
            }
        } else {
            fadeTo(to)
        }
        residualPose = DockletPose.Zero
        settling = false
        if (landingPending) {
            landingPending = false
            land(to)
        }
    }

    private suspend fun fadeTo(to: Float) {
        if (to == 1f) {
            barAlpha.animateTo(0f, tween(ReducedFadeMillis, easing = LinearEasing))
            progress.snapTo(1f)
            barAlpha.snapTo(1f)
        } else {
            progress.snapTo(0f)
            barAlpha.snapTo(0f)
            barAlpha.animateTo(1f, tween(ReducedFadeMillis, easing = LinearEasing))
        }
    }

    private fun land(to: Float) {
        when {
            to == 1f && quiet -> onLanded(DockletLanding.HiddenQuietly)
            to == 1f -> {
                haptics.perform(DockletMoment.Hidden)
                onLanded(DockletLanding.HiddenBySwipe)
            }
            else -> {
                quiet = false
                haptics.perform(DockletMoment.Restored)
                onLanded(DockletLanding.Shown)
            }
        }
    }

    private fun stopMotion(): Float {
        peekJob?.cancel()
        val current = screenProgress().coerceIn(0f, 1f)
        settleJob?.cancel()
        settleJob = null
        settling = false
        residualPose = DockletPose.Zero
        return current
    }

    private fun currentProgress(): Float = if (isHeld) dragProgress else screenProgress()

    private fun screenProgress(): Float = progress.value - DockletDrawbridge.peekDip(peek.value)

    private companion object {
        const val HalfWay = 0.5f
        const val PeekMillis = 400
        const val ReducedFadeMillis = 220
        val PeekDelay = 700.milliseconds
    }
}

@Composable
fun rememberDockletHideState(onLanded: DockletHideState.(DockletLanding) -> Unit): DockletHideState {
    val scope = rememberCoroutineScope()
    val haptics = rememberDockletHaptics()
    val currentOnLanded by rememberUpdatedState(onLanded)
    return remember(scope, haptics) {
        DockletHideState(
            scope = scope,
            haptics = haptics,
            animationsEnabled = ValueAnimator::areAnimatorsEnabled,
            onLanded = { currentOnLanded(this, it) },
        )
    }
}
