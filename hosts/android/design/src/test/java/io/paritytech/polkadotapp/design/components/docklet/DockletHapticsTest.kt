package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertEquals
import org.junit.Test

class DockletHapticsTest {
    @Test
    fun `open plays CONFIRM when the API level is 30`() {
        assertEquals(CONFIRM, dockletHapticConstant(DockletMoment.Open, sdkInt = 30))
    }

    @Test
    fun `open plays CONTEXT_CLICK when the API level is 29`() {
        assertEquals(CONTEXT_CLICK, dockletHapticConstant(DockletMoment.Open, sdkInt = 29))
    }

    @Test
    fun `swipe start plays CLOCK_TICK when the swipe-up starts moving the card`() {
        assertEquals(CLOCK_TICK, dockletHapticConstant(DockletMoment.SwipeStart, sdkInt = 35))
    }

    @Test
    fun `close plays CLOCK_TICK when the card returns to rest`() {
        assertEquals(CLOCK_TICK, dockletHapticConstant(DockletMoment.Close, sdkInt = 35))
    }

    @Test
    fun `step change plays CONTEXT_CLICK when the card changes step`() {
        assertEquals(CONTEXT_CLICK, dockletHapticConstant(DockletMoment.StepChange, sdkInt = 35))
    }

    @Test
    fun `slot tap plays CLOCK_TICK when the API level is 29`() {
        assertEquals(CLOCK_TICK, dockletHapticConstant(DockletMoment.SlotTap, sdkInt = 29))
    }

    @Test
    fun `slot tap plays CLOCK_TICK when the API level is 35`() {
        assertEquals(CLOCK_TICK, dockletHapticConstant(DockletMoment.SlotTap, sdkInt = 35))
    }

    private companion object {
        const val CONFIRM = 16
        const val CONTEXT_CLICK = 6
        const val CLOCK_TICK = 4
    }
}
