package io.paritytech.polkadotapp.feature_products_api.model

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class JsMaskShapesTest {
    private val circle = JsMaskShape.Circle(JsMaskPoint(0, 0), 300)
    private val stroke = JsMaskShape.Stroke(5200, listOf(JsMaskPoint(500, 500), JsMaskPoint(900, 500)))
    private val revealed = JsModifier.Mask(key = "mask", shapes = listOf(JsMaskShape.All), fadeMs = 350)

    @Test
    fun `shapes add up until a tree, and an empty extension changes nothing`() {
        val channel = JsMaskShapes()

        channel.extend(listOf(JsMaskShape.All))
        channel.extend(emptyList())
        channel.extend(listOf(circle))
        assertEquals(listOf(JsMaskShape.All, circle), channel.added)

        channel.treeArrived(revealed)
        assertEquals(emptyList<JsMaskShape>(), channel.added)
    }

    @Test
    fun `a tree that changes the mask fades from what was drawn, strokes included`() {
        val channel = JsMaskShapes()
        val drawn = JsMaskShapes.Picture(JsMaskMode.HIDE, listOf(circle, stroke))
        channel.source = { drawn }

        channel.treeArrived(revealed, nowNanos = 1_000)

        val fade = channel.fade
        assertNotNull(fade)
        assertSame(drawn, fade!!.from)
        assertEquals(1_000L, fade.startNanos)
        assertEquals(350_000_000L, fade.durationNanos)
        channel.endFade(nowNanos = 200_000_000)
        assertNotNull(channel.fade)
        channel.endFade(nowNanos = 350_001_000)
        assertNull(channel.fade)
    }

    @Test
    fun `no fade without a duration, nor when the tree draws what was drawn, and a running fade goes on`() {
        val channel = JsMaskShapes()
        channel.source = { JsMaskShapes.Picture(JsMaskMode.HIDE, emptyList()) }
        channel.treeArrived(revealed.copy(fadeMs = 0), nowNanos = 1)
        assertNull(channel.fade)

        channel.treeArrived(revealed, nowNanos = 2)
        val running = channel.fade
        assertNotNull(running)
        channel.source = { JsMaskShapes.Picture(JsMaskMode.HIDE, listOf(JsMaskShape.All)) }
        channel.treeArrived(revealed, nowNanos = 3)
        assertSame("the same mask again leaves the fade running", running, channel.fade)

        channel.treeArrived(null, nowNanos = 4)
        assertNull("a tree without the mask drops its fade", channel.fade)
    }

    @Test
    fun `a mask never drawn has nothing to fade from`() {
        val channel = JsMaskShapes()

        channel.treeArrived(revealed)

        assertNull(channel.fade)
    }

    @Test
    fun `a tree's keyed masks are found at any depth, the last of a node and the first of a key winning`() {
        val inner = JsModifier.Mask(key = "a", shapes = listOf(JsMaskShape.All))
        val tree = JsWidget.Column(
            modifiers = listOf(JsModifier.Mask(key = "b", mode = JsMaskMode.SHOW), JsModifier.Mask(key = "c")),
            children = listOf(
                JsWidget.Box(children = listOf(JsWidget.Spacer(modifiers = listOf(inner)))),
                JsWidget.Spacer(modifiers = listOf(JsModifier.Mask(key = "a"))),
                JsWidget.Spacer(modifiers = listOf(JsModifier.Mask())),
            ),
        )

        assertEquals(mapOf("c" to JsModifier.Mask(key = "c"), "a" to inner), tree.keyedMasks())
    }
}

class JsPanTrailTest {
    @Test
    fun `strokes begin at touch-down, skip repeated places, and a tree drops them`() {
        val trail = JsPanTrail()

        trail.extend(Offset(1f, 1f))
        trail.extend(Offset(1f, 1f))
        trail.begin(Offset(5f, 5f))
        trail.extend(Offset(6f, 5f))
        assertEquals(listOf(listOf(Offset(1f, 1f)), listOf(Offset(5f, 5f), Offset(6f, 5f))), trail.strokes)

        trail.reset()
        assertEquals(emptyList<List<Offset>>(), trail.strokes)
    }
}
