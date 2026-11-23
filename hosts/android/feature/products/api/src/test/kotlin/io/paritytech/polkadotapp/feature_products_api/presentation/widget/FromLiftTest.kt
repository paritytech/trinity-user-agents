package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FromLiftTest {
    private val from = AnimationVector1D(120f)
    private val to = AnimationVector1D(0f)
    private val still = AnimationVector1D(0f)

    @Test
    fun `a release timed from the lift is already a frame in on its first frame`() {
        val reference = spring<Float>(dampingRatio = 0.8f, stiffness = 324f).vectorize(Float.VectorConverter)
        val lifted = spring<Float>(dampingRatio = 0.8f, stiffness = 324f).fromLift().vectorize(Float.VectorConverter)
        val first = lifted.getValueFromNanos(0L, from, to, still).value
        assertEquals(reference.getValueFromNanos(LIFT_FRAME_NANOS, from, to, still).value, first, 0.001f)
        assertTrue("it has left the place it was let go at", first < 120f)
        assertEquals(
            reference.getDurationNanos(from, to, still) - LIFT_FRAME_NANOS,
            lifted.getDurationNanos(from, to, still),
        )
    }

    @Test
    fun `the curve is the same, one frame earlier`() {
        val reference = tween<Float>(117, easing = WidgetStyleRules.EaseOutQuad).vectorize(Float.VectorConverter)
        val lifted = tween<Float>(117, easing = WidgetStyleRules.EaseOutQuad).fromLift().vectorize(Float.VectorConverter)
        for (frame in 0..6) {
            val at = frame * LIFT_FRAME_NANOS
            assertEquals(
                reference.getValueFromNanos(at + LIFT_FRAME_NANOS, from, to, still).value,
                lifted.getValueFromNanos(at, from, to, still).value,
                0.001f,
            )
        }
    }
}
