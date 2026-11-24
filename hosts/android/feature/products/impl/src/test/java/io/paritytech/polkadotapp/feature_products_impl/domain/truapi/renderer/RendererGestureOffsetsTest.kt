package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.BoxProps
import uniffi.truapi.CoverBrush
import uniffi.truapi.CoverGridProps
import uniffi.truapi.GestureProps
import uniffi.truapi.PanAxis
import uniffi.truapi.PanGesture
import uniffi.truapi.RendererNode

class RendererGestureOffsetsTest {
    @Test
    fun `position updates do not emit a tree and preserve signed coordinates`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("thumb")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        val answer = position.value.value.answer

        assertNull(offsets.receive(RendererNode.SetOffset("thumb", 73, -4)))
        assertEquals(JsGesturePosition.Value(x = 73, y = -4, answer = answer + 1, attached = true), position.value.value)
    }

    @Test
    fun `an unchanged position still acknowledges another move`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("thumb")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        offsets.receive(RendererNode.SetOffset("thumb", 5, 0))
        val answer = position.value.value.answer

        offsets.receive(RendererNode.SetOffset("thumb", 5, 0))

        assertEquals(answer + 1, position.value.value.answer)
    }

    @Test
    fun `a full tree resets offsets and acknowledges the outstanding move without replacing the binding`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(gesture("thumb")) as JsWidget.Gesture
        val position = requireNotNull(first.position)
        offsets.receive(RendererNode.SetOffset("thumb", 50, 60))
        val answer = position.value.value.answer

        val next = offsets.receive(gesture("thumb")) as JsWidget.Gesture

        assertSame(position, next.position)
        assertEquals(JsGesturePosition.Value(x = 0, y = 0, answer = answer + 1, attached = true), position.value.value)
    }

    @Test
    fun `an update for an unknown key cannot affect a future tree`() {
        val offsets = RendererGestureOffsets()
        offsets.receive(RendererNode.SetOffset("thumb", 99, 99))

        val gesture = offsets.receive(gesture("thumb")) as JsWidget.Gesture

        assertEquals(0, requireNotNull(gesture.position).value.value.x)
    }

    @Test
    fun `removing a key detaches its pending gesture and readding it starts at zero`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(gesture("thumb")) as JsWidget.Gesture
        offsets.receive(RendererNode.Nil)
        offsets.receive(RendererNode.SetOffset("thumb", 99, 99))

        val next = offsets.receive(gesture("thumb")) as JsWidget.Gesture

        assertFalse(requireNotNull(first.position).value.value.attached)
        assertNotSame(first.position, next.position)
        assertEquals(0, requireNotNull(next.position).value.value.x)
    }

    @Test
    fun `render contexts never share a key's position`() {
        val left = RendererGestureOffsets()
        val right = RendererGestureOffsets()
        left.receive(gesture("thumb"))
        val untouched = right.receive(gesture("thumb")) as JsWidget.Gesture

        left.receive(RendererNode.SetOffset("thumb", 80, 0))

        assertEquals(0, requireNotNull(untouched.position).value.value.x)
    }

    @Test
    fun `nested keys bind but an offset nested in a tree does not draw or change the binding`() {
        val offsets = RendererGestureOffsets()
        val tree = RendererNode.Box(
            modifiers = emptyList(),
            props = BoxProps(contentAlignment = null),
            children = listOf(gesture("thumb"), RendererNode.SetOffset("thumb", 99, 99)),
        )

        val box = offsets.receive(tree) as JsWidget.Box
        val position = requireNotNull((box.children.single() as JsWidget.Gesture).position)

        assertEquals(0, position.value.value.x)
        offsets.receive(RendererNode.SetOffset("thumb", 30, 0))
        assertEquals(30, position.value.value.x)
    }

    @Test
    fun `closing the stream stops pending moves but keeps its final visible position`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("thumb")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        offsets.receive(RendererNode.SetOffset("thumb", 40, 0))
        val before = position.value.value

        offsets.close()
        offsets.receive(RendererNode.SetOffset("thumb", 99, 99))

        assertEquals(before.copy(attached = false), position.value.value)
    }

    @Test
    fun `cleared cells reach their cover, emit no tree and answer the pan under way`() {
        val offsets = RendererGestureOffsets()
        val box = offsets.receive(scratch()) as JsWidget.Box
        val gesture = box.children[0] as JsWidget.Gesture
        val cover = box.children[1] as JsWidget.CoverGrid
        val position = requireNotNull(gesture.position)
        val cells = requireNotNull(cover.cells)
        val answer = position.value.value.answer

        assertNull(offsets.receive(RendererNode.ClearCells("foil", listOf(3u.toUShort(), 4u.toUShort()))))

        assertEquals(listOf(3, 4), cells.value.value.cells.stream().toArray().toList())
        assertEquals(answer + 1, position.value.value.answer)
        assertEquals(0, position.value.value.x)
    }

    @Test
    fun `a tree after cleared cells wins and keeps the same cover`() {
        val offsets = RendererGestureOffsets()
        val first = (offsets.receive(scratch()) as JsWidget.Box).children[1] as JsWidget.CoverGrid
        offsets.receive(RendererNode.ClearCells("foil", listOf(7u.toUShort())))

        val next = (offsets.receive(scratch()) as JsWidget.Box).children[1] as JsWidget.CoverGrid

        assertSame(first.cells, next.cells)
        assertTrue(requireNotNull(next.cells).value.value.cells.isEmpty)
    }

    @Test
    fun `cells outside any grid and unknown covers are ignored`() {
        val offsets = RendererGestureOffsets()
        val cover = (offsets.receive(scratch()) as JsWidget.Box).children[1] as JsWidget.CoverGrid
        offsets.receive(RendererNode.ClearCells("other", listOf(1u.toUShort())))
        offsets.receive(RendererNode.ClearCells("foil", listOf(65535u.toUShort())))

        assertTrue(requireNotNull(cover.cells).value.value.cells.isEmpty)
    }

    @Test
    fun `a cover's brush shares the strokes of the pan it follows`() {
        val offsets = RendererGestureOffsets()
        val box = offsets.receive(scratch()) as JsWidget.Box
        val gesture = box.children[0] as JsWidget.Gesture
        val brush = requireNotNull((box.children[1] as JsWidget.CoverGrid).brush)

        assertSame(gesture.trail, brush.trail)
        assertEquals(52, brush.width)
        assertEquals("scratch", brush.pan)
    }

    @Test
    fun `a tree drops the strokes with the cells cleared since, and keeps the same trail`() {
        val offsets = RendererGestureOffsets()
        val gesture = (offsets.receive(scratch()) as JsWidget.Box).children[0] as JsWidget.Gesture
        val trail = requireNotNull(gesture.trail)
        trail.begin(1f, 2f)
        trail.extend(30f, 2f)

        val next = (offsets.receive(scratch()) as JsWidget.Box).children[0] as JsWidget.Gesture

        assertSame(trail, next.trail)
        assertTrue(trail.value.value.strokes.isEmpty())
    }

    private fun scratch() = RendererNode.Box(
        modifiers = emptyList(),
        props = BoxProps(contentAlignment = null),
        children = listOf(
            gesture("scratch"),
            RendererNode.CoverGrid(
                modifiers = emptyList(),
                props = CoverGridProps(
                    key = "foil",
                    columns = 4u,
                    rows = 4u,
                    cleared = ByteArray(0),
                    brush = CoverBrush(pan = "scratch", width = 52uL),
                    reveal = null,
                ),
                children = emptyList(),
            ),
        ),
    )

    private fun gesture(key: String) = RendererNode.Gesture(
        modifiers = emptyList(),
        props = GestureProps(
            tapAction = null,
            pan = PanGesture(
                axis = PanAxis.FREE,
                drag = null,
                startAction = null,
                moveAction = "move",
                moveIntervalMs = null,
                endAction = null,
                location = true,
            ),
            accessibilityLabel = null,
            enabled = null,
            hapticOnComplete = null,
            key = key,
            feel = null,
        ),
        children = emptyList(),
    )
}
