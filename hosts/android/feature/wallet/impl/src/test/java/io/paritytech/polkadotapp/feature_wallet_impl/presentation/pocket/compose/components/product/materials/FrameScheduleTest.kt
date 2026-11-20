package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameScheduleTest {
    private val schedule = FrameSchedule().apply { resume(nowNanos = 0) }

    @Test
    fun `nothing draws while paused or after release`() {
        schedule.pause()
        schedule.onChange()
        assertFalse(schedule.next(animating = true))

        schedule.resume(nowNanos = 0)
        schedule.release()
        schedule.resume(nowNanos = 0)
        assertFalse(schedule.next(animating = true))
    }

    // The RFC's always-on step: a node that reads neither time nor motion is drawn once, not on a loop.
    @Test
    fun `a still node draws once per change`() {
        assertTrue(schedule.next(animating = false))
        assertFalse(schedule.next(animating = false))
        assertFalse(schedule.wantsFrame(animating = false))

        schedule.onChange()
        assertTrue(schedule.next(animating = false))
        assertFalse(schedule.wantsFrame(animating = false))
    }

    @Test
    fun `the material clock restarts when the node is attached or resumed`() {
        assertEquals(2f, schedule.seconds(nowNanos = 2_000_000_000), 0f)

        schedule.pause()
        schedule.resume(nowNanos = 5_000_000_000)
        assertEquals(0f, schedule.seconds(nowNanos = 5_000_000_000), 0f)

        schedule.resume(nowNanos = 9_000_000_000)
        assertEquals(1f, schedule.seconds(nowNanos = 10_000_000_000), 0f)
    }
}
