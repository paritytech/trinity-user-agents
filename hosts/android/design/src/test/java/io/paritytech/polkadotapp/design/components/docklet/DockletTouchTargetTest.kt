package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Test

class DockletTouchTargetTest {
    private val dock = DockletFrame(10.dp, 785.dp, 382.dp, 62.dp)
    private val pill = DockletFrame(169.dp, 775.dp, 64.dp, 64.dp)
    private val edgeSliver = DockletFrame(392.dp, 785.dp, 10.dp, 62.dp)
    private val grabZone = DockletFrame(355.dp, 420.dp, 47.dp, 200.dp)
    private val areas = listOf(dock, pill, edgeSliver, grabZone)

    @Test
    fun `a gap between two slots is on the Docklet`() {
        assertEquals(DockletTouch.Docklet, dockletTouch(DockletPoint(90f, 816f), areas))
    }

    @Test
    fun `the Pill above the dock is on the Docklet`() {
        assertEquals(DockletTouch.Docklet, dockletTouch(DockletPoint(201f, 778f), areas))
    }

    @Test
    fun `the edge sliver beside the dock and the glow's grab zone are on the Docklet`() {
        assertEquals(DockletTouch.Docklet, dockletTouch(DockletPoint(398f, 816f), areas))
        assertEquals(DockletTouch.Docklet, dockletTouch(DockletPoint(380f, 500f), areas))
    }

    @Test
    fun `tab content is outside`() {
        assertEquals(DockletTouch.Outside, dockletTouch(DockletPoint(200f, 400f), areas))
        assertEquals(DockletTouch.Outside, dockletTouch(DockletPoint(200f, 770f), areas))
    }
}
