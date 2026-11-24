package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsAnimatedProperty
import io.paritytech.polkadotapp.feature_products_api.model.JsEnter
import io.paritytech.polkadotapp.feature_products_api.model.JsMotion
import io.paritytech.polkadotapp.feature_products_api.model.JsTrack
import io.paritytech.polkadotapp.feature_products_api.model.JsTrackPlay

internal data class NodePose(val opacity: Float, val scale: Float, val x: Float, val y: Float, val rotation: Float)
internal data class NodeMotionFrame(val pose: NodePose, val running: Boolean)

/** One node appearance, with a separate clock for each property; replacing a tree does not replace these clocks. */
internal class NodeMotionPlayer(private val enter: JsEnter?, tracks: List<JsTrack>) {
    private var born: Long? = null
    private var lastFrame: Long? = null
    private var enterDone = enter == null
    private val clocks = linkedMapOf<JsAnimatedProperty, TrackClock>()

    init {
        update(tracks)
    }

    fun update(tracks: List<JsTrack>) {
        val latest = tracks.associateBy { it.property }
        clocks.keys.retainAll(latest.keys)
        latest.forEach { (property, track) ->
            val clock = clocks[property]
            if (clock == null) clocks[property] = TrackClock(track.play) else clock.update(track.play, lastFrame)
        }
    }

    val hasEnter: Boolean get() = enter != null

    fun initialPose(animate: Boolean): NodePose {
        if (!animate) enterDone = true
        return combine(if (enterDone) 1.0 else 0.0, clocks.mapValues { it.value.initial(animate) })
    }

    /** The next tree's pose without starting a new timeline or following leg before its first frame. */
    fun preview(animate: Boolean): NodePose {
        if (!animate) return initialPose(animate = false)
        val now = lastFrame
        val start = born
        val progress = when {
            enter == null || enterDone -> 1.0
            now == null || start == null -> 0.0
            else -> NodeMotionRules.motion(enter.motion, 0.0, 1.0, 0.0, now - start - enter.delayMs * 1_000_000L).value
        }
        return combine(progress, clocks.mapValues { it.value.preview(now) })
    }

    fun sample(nowNanos: Long, animate: Boolean): NodeMotionFrame {
        val start = born ?: nowNanos.also { born = it }
        lastFrame = nowNanos
        if (!animate) enterDone = true
        val entrance = if (enter == null || enterDone) {
            MotionSample(1.0, 0.0, true)
        } else {
            NodeMotionRules.motion(enter.motion, 0.0, 1.0, 0.0, nowNanos - start - enter.delayMs * 1_000_000L)
        }
        if (entrance.finished) enterDone = true
        val samples = clocks.mapValues { it.value.sample(nowNanos, animate) }
        return NodeMotionFrame(
            combine(entrance.value, samples.mapValues { it.value.value }),
            !entrance.finished || samples.values.any { !it.finished },
        )
    }

    private fun combine(progress: Double, values: Map<JsAnimatedProperty, Double?>): NodePose {
        val from = enter?.from
        fun enterValue(initial: Double, target: Double) = initial + (target - initial) * progress
        val opacity = enterValue((from?.opacityPermille ?: 1000).coerceIn(0, 1000) / 1000.0, 1.0)
        val scale = enterValue((from?.scalePermille ?: 1000) / 1000.0, 1.0)
        return NodePose(
            (opacity * (values[JsAnimatedProperty.OPACITY] ?: 1000.0).coerceIn(0.0, 1000.0) / 1000).coerceIn(0.0, 1.0).toFloat(),
            (scale * (values[JsAnimatedProperty.SCALE] ?: 1000.0) / 1000).toFloat(),
            ((enterValue((from?.x ?: 0).toDouble(), 0.0) + (values[JsAnimatedProperty.OFFSET_X] ?: 0.0)) / 100).toFloat(),
            ((enterValue((from?.y ?: 0).toDouble(), 0.0) + (values[JsAnimatedProperty.OFFSET_Y] ?: 0.0)) / 100).toFloat(),
            ((enterValue((from?.rotationCentideg ?: 0).toDouble(), 0.0) + (values[JsAnimatedProperty.ROTATION] ?: 0.0)) / 100).toFloat(),
        )
    }
}

private class TrackClock(private var play: JsTrackPlay) {
    private var start: Long? = null
    private var timeline = (play as? JsTrackPlay.Keyframes)?.let(::NodeTimeline)
    private var from = (play as? JsTrackPlay.Follow)?.target?.toDouble() ?: 0.0
    private var velocity = 0.0
    private var leg: JsMotion? = null

    fun initial(animate: Boolean): Double? = when (val current = play) {
        is JsTrackPlay.Follow -> {
            if (!animate) leg = null
            current.target.toDouble()
        }
        is JsTrackPlay.Keyframes -> timeline?.sample(0, animate)?.value
    }

    fun update(next: JsTrackPlay, lastFrame: Long?) {
        if (next == play) return
        val previous = play
        if (next is JsTrackPlay.Follow && previous is JsTrackPlay.Follow) {
            if (lastFrame == null) {
                from = next.target.toDouble()
                leg = null
                start = null
            } else if (next.target != previous.target) {
                val drawn = sample(lastFrame ?: 0, animate = true)
                from = requireNotNull(drawn.value)
                velocity = if (next.motion is JsMotion.Spring) drawn.velocity else 0.0
                leg = next.motion
                start = null
            }
        } else {
            start = null
            leg = null
            velocity = 0.0
            from = (next as? JsTrackPlay.Follow)?.target?.toDouble() ?: 0.0
            timeline = (next as? JsTrackPlay.Keyframes)?.let(::NodeTimeline)
        }
        play = next
    }

    fun preview(now: Long?): Double? = when (val current = play) {
        is JsTrackPlay.Keyframes -> timeline?.sample(if (now == null || start == null) 0 else (now - requireNotNull(start)) / 1_000_000, true)?.value
        is JsTrackPlay.Follow -> {
            val motion = leg
            when {
                motion == null -> current.target.toDouble()
                now == null || start == null -> from
                else -> NodeMotionRules.motion(motion, from, current.target.toDouble(), velocity, now - requireNotNull(start)).value
            }
        }
    }

    fun sample(nowNanos: Long, animate: Boolean): TrackSample {
        val began = start ?: nowNanos.also { start = it }
        return when (val current = play) {
            is JsTrackPlay.Keyframes -> {
                val sample = requireNotNull(timeline).sample((nowNanos - began) / 1_000_000, animate)
                TrackSample(sample.value, 0.0, sample.finished)
            }
            is JsTrackPlay.Follow -> {
                val motion = leg
                if (!animate || motion == null) {
                    leg = null
                    TrackSample(current.target.toDouble(), 0.0, true)
                } else {
                    val sample = NodeMotionRules.motion(motion, from, current.target.toDouble(), velocity, nowNanos - began)
                    if (sample.finished) leg = null
                    TrackSample(sample.value, sample.velocity, sample.finished)
                }
            }
        }
    }
}

private data class TrackSample(val value: Double?, val velocity: Double, val finished: Boolean)
