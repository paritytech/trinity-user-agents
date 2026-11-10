package io.paritytech.polkadotapp.design.components.docklet

import androidx.activity.BackEventCompat
import org.junit.Assert.assertEquals
import org.junit.Test

class DockletBackTest {
    @Test
    fun `a right-edge swipe inside the band pulls the hidden dock`() {
        assertEquals(DockletBackAction.Claim, dockletBackAction(DockletBackEdge.Right, 700f, Band, restoresOnBack = true))
    }

    @Test
    fun `a right-edge swipe outside the band is a plain Back`() {
        assertEquals(DockletBackAction.PlainBack, dockletBackAction(DockletBackEdge.Right, 200f, Band, restoresOnBack = true))
    }

    @Test
    fun `a left-edge swipe is a plain Back and leaves the dock hidden`() {
        assertEquals(DockletBackAction.PlainBack, dockletBackAction(DockletBackEdge.Left, 700f, Band, restoresOnBack = true))
    }

    @Test
    fun `a Back with no swipe edge brings the hidden dock back`() {
        assertEquals(DockletBackAction.Restore, dockletBackAction(DockletBackEdge.None, 0f, Band, restoresOnBack = true))
    }

    @Test
    fun `over a product a Back with no swipe edge goes to the product`() {
        assertEquals(DockletBackAction.PlainBack, dockletBackAction(DockletBackEdge.None, 0f, Band, restoresOnBack = false))
        assertEquals(DockletBackAction.PlainBack, dockletPressBackAction(restoresOnBack = false))
    }

    @Test
    fun `over a product a right-edge swipe inside the band still pulls the hidden dock`() {
        assertEquals(DockletBackAction.Claim, dockletBackAction(DockletBackEdge.Right, 700f, Band, restoresOnBack = false))
    }

    @Test
    fun `on a tab a Back that never reported an edge brings the hidden dock back`() {
        assertEquals(DockletBackAction.Restore, dockletPressBackAction(restoresOnBack = true))
    }

    @Test
    fun `from Android 15 a left swipe is a left edge`() {
        assertEquals(DockletBackEdge.Left, dockletBackEdge(Left, sdkInt = 35))
        assertEquals(DockletBackEdge.Right, dockletBackEdge(Right, sdkInt = 35))
        assertEquals(DockletBackEdge.None, dockletBackEdge(NoEdge, sdkInt = 36))
    }

    @Test
    fun `before Android 15 the left edge also stands for the Back button, so it counts as no edge`() {
        assertEquals(DockletBackEdge.None, dockletBackEdge(Left, sdkInt = 34))
        assertEquals(DockletBackEdge.Right, dockletBackEdge(Right, sdkInt = 34))
    }

    private companion object {
        val Band = 396f..871f
        const val Left = BackEventCompat.EDGE_LEFT
        const val Right = BackEventCompat.EDGE_RIGHT
        const val NoEdge = 2
    }
}
