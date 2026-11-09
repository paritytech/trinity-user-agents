package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertEquals
import org.junit.Test

class DockletBackTest {
    @Test
    fun `a right-edge swipe inside the band pulls the hidden dock`() {
        assertEquals(DockletBackAction.Claim, dockletBackAction(DockletBackEdge.Right, 700f, Band))
    }

    @Test
    fun `the band reaches 24 dp past both its ends`() {
        assertEquals(DockletBackAction.Claim, dockletBackAction(DockletBackEdge.Right, 396f, Band))
        assertEquals(DockletBackAction.Claim, dockletBackAction(DockletBackEdge.Right, 871f, Band))
    }

    @Test
    fun `a right-edge swipe outside the band is a plain Back`() {
        assertEquals(DockletBackAction.PlainBack, dockletBackAction(DockletBackEdge.Right, 200f, Band))
    }

    @Test
    fun `a left-edge swipe is a plain Back and leaves the dock hidden`() {
        assertEquals(DockletBackAction.PlainBack, dockletBackAction(DockletBackEdge.Left, 700f, Band))
    }

    @Test
    fun `a Back with no swipe edge brings the hidden dock back`() {
        assertEquals(DockletBackAction.Restore, dockletBackAction(DockletBackEdge.None, 0f, Band))
    }

    @Test
    fun `from Android 15 a left swipe is a left edge`() {
        assertEquals(DockletBackEdge.Left, dockletBackEdge(Left, Left, Right, sdkInt = 35))
        assertEquals(DockletBackEdge.Right, dockletBackEdge(Right, Left, Right, sdkInt = 35))
        assertEquals(DockletBackEdge.None, dockletBackEdge(NoEdge, Left, Right, sdkInt = 36))
    }

    @Test
    fun `before Android 15 the left edge also stands for the Back button, so it counts as no edge`() {
        assertEquals(DockletBackEdge.None, dockletBackEdge(Left, Left, Right, sdkInt = 34))
        assertEquals(DockletBackEdge.Right, dockletBackEdge(Right, Left, Right, sdkInt = 34))
    }

    @Test
    fun `Back in search goes from typing to scan, then closes`() {
        assertEquals(DockletStep.Step1, dockletSearchBackStep(DockletStep.Step2))
        assertEquals(DockletStep.Rest, dockletSearchBackStep(DockletStep.Step1))
    }

    private companion object {
        val Band = 396f..871f
        const val Left = 0
        const val Right = 1
        const val NoEdge = 2
    }
}
