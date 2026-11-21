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
}
