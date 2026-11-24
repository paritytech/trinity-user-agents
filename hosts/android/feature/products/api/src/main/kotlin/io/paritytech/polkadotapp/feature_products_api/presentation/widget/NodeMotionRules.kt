package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsCurve
import io.paritytech.polkadotapp.feature_products_api.model.JsMotion
import io.paritytech.polkadotapp.feature_products_api.model.JsTrackPlay
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

internal data class MotionSample(val value: Double, val velocity: Double, val finished: Boolean)
internal data class TimelineSample(val value: Double?, val finished: Boolean)

/** Curves and timelines follow the core's sampling rules; springs also carry their current velocity. */
internal object NodeMotionRules {
    fun ease(curve: JsCurve, time: Double): Double {
        val t = time.coerceIn(0.0, 1.0)
        fun bezier(a: Double, b: Double, at: Double): Double {
            val rest = 1 - at
            return 3 * rest * rest * at * a + 3 * rest * at * at * b + at * at * at
        }
        val x1 = curve.x1.coerceIn(0, 1000) / 1000.0
        val x2 = curve.x2.coerceIn(0, 1000) / 1000.0
        var low = 0.0
        var high = 1.0
        repeat(50) {
            val middle = (low + high) / 2
            if (bezier(x1, x2, middle) < t) low = middle else high = middle
        }
        return bezier(curve.y1 / 1000.0, curve.y2 / 1000.0, (low + high) / 2)
    }

    fun motion(motion: JsMotion, from: Double, target: Double, velocity: Double, elapsedNanos: Long): MotionSample {
        if (elapsedNanos < 0) return MotionSample(from, 0.0, false)
        val seconds = elapsedNanos / 1_000_000_000.0
        return when (motion) {
            is JsMotion.Timed -> {
                val duration = motion.ms / 1000.0
                if (duration <= 0 || seconds >= duration) {
                    MotionSample(target, 0.0, true)
                } else {
                    val time = seconds / duration
                    val low = (time - 0.0001).coerceAtLeast(0.0)
                    val high = (time + 0.0001).coerceAtMost(1.0)
                    val speed = (ease(motion.curve, high) - ease(motion.curve, low)) / ((high - low) * duration)
                    MotionSample(from + (target - from) * ease(motion.curve, time), (target - from) * speed, false)
                }
            }
            is JsMotion.Spring -> spring(motion, from, target, velocity, seconds)
        }
    }

    private fun spring(motion: JsMotion.Spring, from: Double, target: Double, velocity: Double, seconds: Double): MotionSample {
        val omega = motion.omegaCenti / 100.0
        if (omega <= 0) return MotionSample(target, 0.0, true)
        val damping = motion.dampingCenti / 100.0
        val initial = from - target
        val displacement: Double
        val speed: Double
        when {
            abs(damping - 1) < 0.00001 -> {
                val coefficient = velocity + omega * initial
                val decay = exp(-omega * seconds)
                displacement = decay * (initial + coefficient * seconds)
                speed = decay * (coefficient - omega * (initial + coefficient * seconds))
            }
            damping < 1 -> {
                val decayRate = omega * damping
                val frequency = omega * sqrt(1 - damping * damping)
                val coefficient = (velocity + decayRate * initial) / frequency
                val wave = initial * cos(frequency * seconds) + coefficient * sin(frequency * seconds)
                val decay = exp(-decayRate * seconds)
                displacement = decay * wave
                speed = decay * (-decayRate * wave + frequency * (-initial * sin(frequency * seconds) + coefficient * cos(frequency * seconds)))
            }
            else -> {
                val root = sqrt(damping * damping - 1)
                val first = -omega * (damping - root)
                val second = -omega * (damping + root)
                val coefficient = (velocity - second * initial) / (first - second)
                val a = coefficient * exp(first * seconds)
                val b = (initial - coefficient) * exp(second * seconds)
                displacement = a + b
                speed = first * a + second * b
            }
        }
        val tolerance = max(max(abs(initial), abs(velocity) / omega) * 0.001, 0.000000001)
        val finished = abs(displacement) <= tolerance && abs(speed) / 60 <= tolerance
        return if (finished) MotionSample(target, 0.0, true) else MotionSample(target + displacement, speed, false)
    }
}

internal class NodeTimeline(private val play: JsTrackPlay.Keyframes) {
    private val frames = buildList {
        var latest = 0L
        play.frames.take(32).forEach { frame ->
            latest = max(latest, frame.atMs)
            add(frame.copy(atMs = latest))
        }
    }

    fun sample(elapsedMs: Long, animate: Boolean): TimelineSample {
        if (frames.isEmpty()) return TimelineSample(null, true)
        if (!animate && play.repeat == 0) return TimelineSample(frames.first().value.toDouble(), true)
        val duration = frames.last().atMs
        if (duration == 0L) return TimelineSample(frames.last().value.toDouble(), true)
        val elapsed = if (animate) elapsedMs.coerceAtLeast(0) else Long.MAX_VALUE
        val completed = play.repeat != 0 && elapsed / duration >= play.repeat
        val run = if (completed) play.repeat.toLong() - 1 else elapsed / duration
        val within = if (completed) duration else elapsed % duration
        val at = if (play.alternate && run % 2 == 1L) duration - within else within
        val exact = frames.lastOrNull { it.atMs == at }
        if (exact != null) return TimelineSample(exact.value.toDouble(), completed || !animate)
        if (at < frames.first().atMs) return TimelineSample(frames.first().value.toDouble(), completed || !animate)
        val next = frames.indexOfFirst { it.atMs > at }
        val from = frames[next - 1]
        val to = frames[next]
        val progress = NodeMotionRules.ease(to.curve, (at - from.atMs).toDouble() / (to.atMs - from.atMs))
        return TimelineSample(from.value + (to.value.toDouble() - from.value) * progress, completed || !animate)
    }
}
