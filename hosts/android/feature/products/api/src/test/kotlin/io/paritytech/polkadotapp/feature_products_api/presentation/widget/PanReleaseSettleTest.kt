package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PanReleaseSettleTest {
    @Test
    fun `a release short of 90 percent springs home`() {
        assertEquals(PanReleaseSettle.Kind.HOME, PanReleaseSettle.kind(JsPanAxis.HORIZONTAL, 899, cancelled = false))
        assertEquals(PanReleaseSettle.Kind.HOME, PanReleaseSettle.kind(JsPanAxis.HORIZONTAL, 0, cancelled = false))
    }

    @Test
    fun `a release at 90 percent or more eases to the end`() {
        assertEquals(PanReleaseSettle.Kind.END, PanReleaseSettle.kind(JsPanAxis.HORIZONTAL, 900, cancelled = false))
        assertEquals(PanReleaseSettle.Kind.END, PanReleaseSettle.kind(JsPanAxis.VERTICAL, 1000, cancelled = false))
    }

    @Test
    fun `a drag taken away or a free pan never settles at the end`() {
        assertEquals(PanReleaseSettle.Kind.HOME, PanReleaseSettle.kind(JsPanAxis.HORIZONTAL, 1000, cancelled = true))
        assertEquals(PanReleaseSettle.Kind.HOME, PanReleaseSettle.kind(JsPanAxis.FREE, 1000, cancelled = false))
    }

    @Test
    fun `the end lies along the pan's axis`() {
        assertEquals(Offset(240f, 0f), PanReleaseSettle.target(PanReleaseSettle.Kind.END, JsPanAxis.HORIZONTAL, 240f))
        assertEquals(Offset(0f, 240f), PanReleaseSettle.target(PanReleaseSettle.Kind.END, JsPanAxis.VERTICAL, 240f))
        assertEquals(Offset.Zero, PanReleaseSettle.target(PanReleaseSettle.Kind.HOME, JsPanAxis.HORIZONTAL, 240f))
    }

    @Test
    fun `nothing animates with animations removed or nowhere to go`() {
        assertTrue(PanReleaseSettle.animates(animatorsEnabled = true, from = Offset(120f, 0f), to = Offset.Zero))
        assertFalse(PanReleaseSettle.animates(animatorsEnabled = false, from = Offset(120f, 0f), to = Offset.Zero))
        assertFalse(PanReleaseSettle.animates(animatorsEnabled = true, from = Offset.Zero, to = Offset.Zero))
    }

    @Test
    fun `the settle is on unless switched off for measuring`() {
        assertTrue(PanReleaseSettle.isOn(null))
        assertFalse(PanReleaseSettle.isOn("0"))
    }

    @Test
    fun `settling children are never drawn past either end of their lane`() {
        assertEquals(Offset(0f, 0f), PanReleaseSettle.onLane(Offset(-4.6f, 0f), 240f))
        assertEquals(Offset(240f, 0f), PanReleaseSettle.onLane(Offset(251f, 0f), 240f))
        assertEquals(Offset(120f, 0f), PanReleaseSettle.onLane(Offset(120f, 0f), 240f))
        assertEquals(Offset.Zero, PanReleaseSettle.onLane(Offset(30f, 0f), 0f))
    }

    @Test
    fun `a release home on a feel's spring dips below home but never passes the end`() {
        assertEquals(Offset(-4.6f, 0f), PanReleaseSettle.onLane(Offset(-4.6f, 0f), 240f, dipsBelowHome = true))
        assertEquals(Offset(240f, 0f), PanReleaseSettle.onLane(Offset(251f, 0f), 240f, dipsBelowHome = true))
        assertEquals(-4.6f, PanReleaseSettle.along(-4.6f, 240f, dipsBelowHome = true), 0f)
        assertEquals(0f, PanReleaseSettle.along(-4.6f, 240f), 0f)
        assertEquals(120f, PanReleaseSettle.along(120f, 240f, dipsBelowHome = true), 0f)
    }
}
