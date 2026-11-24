package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GesturePacingSettingsTest {
    @Test
    fun `sending at the touch is on unless switched off`() {
        assertTrue(GesturePacingSettings.sendOnTouch(null))
        assertTrue(GesturePacingSettings.sendOnTouch(""))
        assertTrue(GesturePacingSettings.sendOnTouch("1"))
        assertFalse(GesturePacingSettings.sendOnTouch("0"))
        assertFalse(GesturePacingSettings.sendOnTouch(" false "))
        assertFalse(GesturePacingSettings.sendOnTouch("off"))
    }

    @Test
    fun `the answer wait defaults to 6 ms and stays between 0 and 10 ms`() {
        assertEquals(6_000_000L, GesturePacingSettings.answerWaitNanos(null))
        assertEquals(6_000_000L, GesturePacingSettings.answerWaitNanos("soon"))
        assertEquals(6_000_000L, GesturePacingSettings.answerWaitNanos("NaN"))
        assertEquals(0L, GesturePacingSettings.answerWaitNanos("0"))
        assertEquals(0L, GesturePacingSettings.answerWaitNanos("-3"))
        assertEquals(2_500_000L, GesturePacingSettings.answerWaitNanos("2.5"))
        assertEquals(10_000_000L, GesturePacingSettings.answerWaitNanos("50"))
    }

    @Test
    fun `moves are at least six tenths of a refresh apart`() {
        assertEquals(10_000_000L, GesturePacingSettings.minimumGapNanos(60f))
        assertEquals(5_000_000L, GesturePacingSettings.minimumGapNanos(120f))
        assertEquals(10_000_000L, GesturePacingSettings.minimumGapNanos(null))
        assertEquals(10_000_000L, GesturePacingSettings.minimumGapNanos(0f))
    }
}
