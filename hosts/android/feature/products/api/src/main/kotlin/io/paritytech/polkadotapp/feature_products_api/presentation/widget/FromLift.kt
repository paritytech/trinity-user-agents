package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.AnimationVector
import androidx.compose.animation.core.TwoWayConverter
import androidx.compose.animation.core.VectorizedAnimationSpec

/**
 * A release animation timed from the finger's lift. An Animatable starts its clock on its first frame and draws the
 * start place there, which holds the released children one frame longer than the lift. This runs the same animation
 * one frame ahead and ends it one frame sooner, so the first frame after the lift already moves them and the curve
 * runs from the lift itself.
 */
internal fun <T> AnimationSpec<T>.fromLift(): AnimationSpec<T> = FromLiftSpec(this)

/** One 60 Hz frame, the first step after the lift. */
internal const val LIFT_FRAME_NANOS = 1_000_000_000L / 60

/** [LIFT_FRAME_NANOS] in ms, for the pop that runs on its own frame clock. */
internal const val LIFT_FRAME_MS = 17

private class FromLiftSpec<T>(private val spec: AnimationSpec<T>) : AnimationSpec<T> {
    override fun <V : AnimationVector> vectorize(converter: TwoWayConverter<T, V>): VectorizedAnimationSpec<V> =
        FromLiftVectorized(spec.vectorize(converter))

    override fun equals(other: Any?): Boolean = other is FromLiftSpec<*> && other.spec == spec

    override fun hashCode(): Int = spec.hashCode() * 31 + 1
}

private class FromLiftVectorized<V : AnimationVector>(private val inner: VectorizedAnimationSpec<V>) : VectorizedAnimationSpec<V> {
    override val isInfinite: Boolean get() = inner.isInfinite

    override fun getValueFromNanos(playTimeNanos: Long, initialValue: V, targetValue: V, initialVelocity: V): V =
        inner.getValueFromNanos(playTimeNanos + LIFT_FRAME_NANOS, initialValue, targetValue, initialVelocity)

    override fun getVelocityFromNanos(playTimeNanos: Long, initialValue: V, targetValue: V, initialVelocity: V): V =
        inner.getVelocityFromNanos(playTimeNanos + LIFT_FRAME_NANOS, initialValue, targetValue, initialVelocity)

    override fun getDurationNanos(initialValue: V, targetValue: V, initialVelocity: V): Long =
        (inner.getDurationNanos(initialValue, targetValue, initialVelocity) - LIFT_FRAME_NANOS).coerceAtLeast(0L)
}
