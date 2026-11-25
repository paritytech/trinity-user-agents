package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

object DockletSpring {
    const val Threshold = 0.0015f
    private const val FastReleaseSpeed = 4f
    private const val MovePeriodSeconds = 0.32
    private const val QuietPeriodSeconds = 0.42
    private const val FlungDamping = 0.78f
    private const val ProjectionSeconds = 0.1f
    private const val HideLine = 0.5f
    private const val TickOn = 0.54f
    private const val TickOff = 0.46f
    private const val FlickSpeed = 350f

    val Move: SpringSpec<Float> = spring(1f, stiffness(MovePeriodSeconds), Threshold)
    val Flung: SpringSpec<Float> = spring(FlungDamping, stiffness(MovePeriodSeconds), Threshold)
    val Quiet: SpringSpec<Float> = spring(1f, stiffness(QuietPeriodSeconds), Threshold)

    fun afterRelease(velocity: Float): SpringSpec<Float> = if (abs(velocity) > FastReleaseSpeed) Flung else Move

    fun projected(progress: Float, velocity: Float): Float = progress + velocity * ProjectionSeconds

    fun hideTarget(startProgress: Float, progress: Float, velocity: Float, turnedBack: Boolean): Float = when {
        turnedBack -> startProgress.roundToInt().toFloat()
        projected(progress, velocity) >= HideLine -> 1f
        else -> 0f
    }

    fun restoreTarget(progress: Float, velocity: Float, fingerSpeed: Float, turnedBack: Boolean): Float =
        if (fingerSpeed > FlickSpeed) 0f else hideTarget(1f, progress, velocity, turnedBack)

    fun pastHalf(wasPast: Boolean, projected: Float): Boolean = projected >= if (wasPast) TickOff else TickOn

    private fun stiffness(periodSeconds: Double): Float = ((2 * PI / periodSeconds) * (2 * PI / periodSeconds)).toFloat()
}

class DockletReleaseTracker(time: Duration, val startProgress: Float, finger: DockletPoint) {
    private val times = LongArray(Capacity)
    private val progresses = FloatArray(Capacity)
    private val xs = FloatArray(Capacity)
    private val ys = FloatArray(Capacity)
    private var count = 0
    private var furthest = 0f
    private var latestProgress = startProgress

    init {
        add(time, startProgress, finger)
    }

    fun add(time: Duration, progress: Float, finger: DockletPoint) {
        val slot = count % Capacity
        times[slot] = time.inWholeMilliseconds
        progresses[slot] = progress
        xs[slot] = finger.x
        ys[slot] = finger.y
        count++
        latestProgress = progress
        furthest = maxOf(furthest, abs(progress - startProgress))
    }

    fun turnedBack(): Boolean = furthest > 0f && furthest - abs(latestProgress - startProgress) > TurnBackShare * furthest

    fun velocity(travel: Float): Float {
        val slowest = TrailingSpeed / travel
        var last = count - 1
        while (last - 1 >= oldest() && isSlow(last - 1, last, slowest)) last--
        val first = firstInWindow(last) ?: return 0f
        return (progressAt(last) - progressAt(first)) / seconds(first, last)
    }

    fun fingerSpeed(): Float {
        val last = count - 1
        val first = firstInWindow(last) ?: return 0f
        val distance = hypot(xAt(last) - xAt(first), yAt(last) - yAt(first))
        return distance / seconds(first, last)
    }

    private fun isSlow(earlier: Int, later: Int, slowest: Float): Boolean =
        abs(progressAt(later) - progressAt(earlier)) < slowest * seconds(earlier, later)

    private fun firstInWindow(last: Int): Int? {
        if (last < oldest()) return null
        var first = last
        while (first - 1 >= oldest() && timeAt(last) - timeAt(first - 1) <= Window.inWholeMilliseconds) first--
        return first.takeIf { it < last && timeAt(last) > timeAt(it) }
    }

    private fun oldest() = maxOf(0, count - Capacity)
    private fun timeAt(index: Int) = times[index % Capacity]
    private fun progressAt(index: Int) = progresses[index % Capacity]
    private fun xAt(index: Int) = xs[index % Capacity]
    private fun yAt(index: Int) = ys[index % Capacity]
    private fun seconds(first: Int, last: Int) = (timeAt(last) - timeAt(first)) / MillisPerSecond

    private companion object {
        const val Capacity = 64
        val Window = 100.milliseconds
        const val TrailingSpeed = 150f
        const val TurnBackShare = 0.3f
        const val MillisPerSecond = 1000f
    }
}
