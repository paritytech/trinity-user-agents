package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import org.junit.Assert.assertFalse
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
}
