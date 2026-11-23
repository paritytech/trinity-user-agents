package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkerThumbSettleTest {
    @Test
    fun `a release short of 90 percent springs home`() {
        assertEquals(WorkerThumbSettle.Kind.HOME, WorkerThumbSettle.kind(JsPanAxis.HORIZONTAL, 899, cancelled = false))
        assertEquals(WorkerThumbSettle.Kind.HOME, WorkerThumbSettle.kind(JsPanAxis.HORIZONTAL, 0, cancelled = false))
    }

    @Test
    fun `a release at 90 percent or more eases to the end`() {
        assertEquals(WorkerThumbSettle.Kind.END, WorkerThumbSettle.kind(JsPanAxis.HORIZONTAL, 900, cancelled = false))
        assertEquals(WorkerThumbSettle.Kind.END, WorkerThumbSettle.kind(JsPanAxis.VERTICAL, 1000, cancelled = false))
    }

    @Test
    fun `a drag taken away or a free pan never settles at the end`() {
        assertEquals(WorkerThumbSettle.Kind.HOME, WorkerThumbSettle.kind(JsPanAxis.HORIZONTAL, 1000, cancelled = true))
        assertEquals(WorkerThumbSettle.Kind.HOME, WorkerThumbSettle.kind(JsPanAxis.FREE, 1000, cancelled = false))
    }

    @Test
    fun `the end lies along the pan's axis`() {
        assertEquals(Offset(240f, 0f), WorkerThumbSettle.target(WorkerThumbSettle.Kind.END, JsPanAxis.HORIZONTAL, 240f))
        assertEquals(Offset(0f, 240f), WorkerThumbSettle.target(WorkerThumbSettle.Kind.END, JsPanAxis.VERTICAL, 240f))
        assertEquals(Offset.Zero, WorkerThumbSettle.target(WorkerThumbSettle.Kind.HOME, JsPanAxis.HORIZONTAL, 240f))
    }

    @Test
    fun `nothing animates with animations removed or nowhere to go`() {
        assertTrue(WorkerThumbSettle.animates(animatorsEnabled = true, from = Offset(120f, 0f), to = Offset.Zero))
        assertFalse(WorkerThumbSettle.animates(animatorsEnabled = false, from = Offset(120f, 0f), to = Offset.Zero))
        assertFalse(WorkerThumbSettle.animates(animatorsEnabled = true, from = Offset.Zero, to = Offset.Zero))
    }

    @Test
    fun `the settle is on unless switched off for measuring`() {
        assertTrue(WorkerThumbSettle.isOn(null))
        assertFalse(WorkerThumbSettle.isOn("0"))
    }

    @Test
    fun `a settling thumb is never drawn past either end of its lane`() {
        assertEquals(Offset(0f, 0f), WorkerThumbSettle.onLane(Offset(-4.6f, 0f), 240f))
        assertEquals(Offset(240f, 0f), WorkerThumbSettle.onLane(Offset(251f, 0f), 240f))
        assertEquals(Offset(120f, 0f), WorkerThumbSettle.onLane(Offset(120f, 0f), 240f))
        assertEquals(Offset.Zero, WorkerThumbSettle.onLane(Offset(30f, 0f), 0f))
    }

    @Test
    fun `a release home with the existing slider's motion dips below home but never passes the end`() {
        assertEquals(Offset(-4.6f, 0f), WorkerThumbSettle.onLane(Offset(-4.6f, 0f), 240f, dipsBelowHome = true))
        assertEquals(Offset(240f, 0f), WorkerThumbSettle.onLane(Offset(251f, 0f), 240f, dipsBelowHome = true))
        assertEquals(-4.6f, WorkerThumbSettle.along(-4.6f, 240f, dipsBelowHome = true), 0f)
        assertEquals(0f, WorkerThumbSettle.along(-4.6f, 240f), 0f)
        assertEquals(120f, WorkerThumbSettle.along(120f, 240f, dipsBelowHome = true), 0f)
    }
}
