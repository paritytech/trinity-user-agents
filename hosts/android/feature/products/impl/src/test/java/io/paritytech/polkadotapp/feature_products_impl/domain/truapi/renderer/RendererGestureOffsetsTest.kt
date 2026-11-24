package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskMode
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.BoxProps
import uniffi.truapi.GestureProps
import uniffi.truapi.ImageProps
import uniffi.truapi.ImageSource
import uniffi.truapi.Mask
import uniffi.truapi.MaskFollow
import uniffi.truapi.MaskMode
import uniffi.truapi.MaskShape
import uniffi.truapi.Modifier
import uniffi.truapi.PanAxis
import uniffi.truapi.PanGesture
import uniffi.truapi.RendererNode

class RendererGestureOffsetsTest {
    @Test
    fun `position updates do not emit a tree and preserve signed coordinates`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("piece")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        val answer = position.value.value.answer

        assertNull(offsets.receive(RendererNode.SetOffset("piece", 73, -4)))
        assertEquals(JsGesturePosition.Value(x = 73, y = -4, answer = answer + 1, attached = true), position.value.value)
    }

    @Test
    fun `an unchanged position still acknowledges another move`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("piece")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        offsets.receive(RendererNode.SetOffset("piece", 5, 0))
        val answer = position.value.value.answer

        offsets.receive(RendererNode.SetOffset("piece", 5, 0))

        assertEquals(answer + 1, position.value.value.answer)
    }

    @Test
    fun `a full tree resets offsets and acknowledges the outstanding move without replacing the binding`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(gesture("piece")) as JsWidget.Gesture
        val position = requireNotNull(first.position)
        offsets.receive(RendererNode.SetOffset("piece", 50, 60))
        val answer = position.value.value.answer

        val next = offsets.receive(gesture("piece")) as JsWidget.Gesture

        assertSame(position, next.position)
        assertEquals(JsGesturePosition.Value(x = 0, y = 0, answer = answer + 1, attached = true), position.value.value)
    }

    @Test
    fun `an update for an unknown key cannot affect a future tree`() {
        val offsets = RendererGestureOffsets()
        offsets.receive(RendererNode.SetOffset("piece", 99, 99))

        val gesture = offsets.receive(gesture("piece")) as JsWidget.Gesture

        assertEquals(0, requireNotNull(gesture.position).value.value.x)
    }

    @Test
    fun `removing a key detaches its pending gesture and readding it starts at zero`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(gesture("piece")) as JsWidget.Gesture
        offsets.receive(RendererNode.Nil)
        offsets.receive(RendererNode.SetOffset("piece", 99, 99))

        val next = offsets.receive(gesture("piece")) as JsWidget.Gesture

        assertFalse(requireNotNull(first.position).value.value.attached)
        assertNotSame(first.position, next.position)
        assertEquals(0, requireNotNull(next.position).value.value.x)
    }

    @Test
    fun `render contexts never share a key's position`() {
        val left = RendererGestureOffsets()
        val right = RendererGestureOffsets()
        left.receive(gesture("piece"))
        val untouched = right.receive(gesture("piece")) as JsWidget.Gesture

        left.receive(RendererNode.SetOffset("piece", 80, 0))

        assertEquals(0, requireNotNull(untouched.position).value.value.x)
    }

    @Test
    fun `nested keys bind but an offset nested in a tree does not draw or change the binding`() {
        val offsets = RendererGestureOffsets()
        val tree = RendererNode.Box(
            modifiers = emptyList(),
            props = BoxProps(contentAlignment = null),
            children = listOf(gesture("piece"), RendererNode.SetOffset("piece", 99, 99)),
        )

        val box = offsets.receive(tree) as JsWidget.Box
        val position = requireNotNull((box.children.single() as JsWidget.Gesture).position)

        assertEquals(0, position.value.value.x)
        offsets.receive(RendererNode.SetOffset("piece", 30, 0))
        assertEquals(30, position.value.value.x)
    }

    @Test
    fun `closing the stream stops pending moves but keeps its final visible position`() {
        val offsets = RendererGestureOffsets()
        val gesture = offsets.receive(gesture("piece")) as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        offsets.receive(RendererNode.SetOffset("piece", 40, 0))
        val before = position.value.value

        offsets.close()
        offsets.receive(RendererNode.SetOffset("piece", 99, 99))

        assertEquals(before.copy(attached = false), position.value.value)
    }

    @Test
    fun `mask shapes reach their mask, emit no tree and answer the pan under way, even with none`() {
        val offsets = RendererGestureOffsets()
        val box = offsets.receive(maskedTree()) as JsWidget.Box
        val gesture = box.children[0] as JsWidget.Gesture
        val position = requireNotNull(gesture.position)
        val shapes = requireNotNull(keyedMask(box).channel)
        val answer = position.value.value.answer

        assertNull(offsets.receive(RendererNode.ExtendMask("mask", listOf(MaskShape.All))))
        assertNull(offsets.receive(RendererNode.ExtendMask("mask", emptyList())))

        assertEquals(listOf<JsMaskShape>(JsMaskShape.All), shapes.added)
        assertEquals(answer + 2, position.value.value.answer)
        assertEquals(0, position.value.value.x)
    }

    @Test
    fun `a tree after mask shapes wins, keeps the same mask and trail, and drops the strokes`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(maskedTree()) as JsWidget.Box
        val trail = requireNotNull((first.children[0] as JsWidget.Gesture).trail)
        trail.begin(Offset(1f, 2f))
        offsets.receive(RendererNode.ExtendMask("mask", listOf(MaskShape.All)))

        val next = offsets.receive(maskedTree()) as JsWidget.Box

        assertSame(keyedMask(first).channel, keyedMask(next).channel)
        assertSame(trail, keyedMask(next).trail)
        assertTrue(requireNotNull(keyedMask(next).channel).added.isEmpty())
        assertTrue(trail.strokes.isEmpty())
    }

    @Test
    fun `a tree that changes a fading mask fades from what was drawn`() {
        val offsets = RendererGestureOffsets()
        val first = offsets.receive(maskedTree()) as JsWidget.Box
        val channel = requireNotNull(keyedMask(first).channel)
        val drawn = JsMaskShapes.Picture(JsMaskMode.HIDE, listOf(JsMaskShape.Stroke(5200, listOf(JsMaskPoint(1, 2)))))
        channel.source = { drawn }

        offsets.receive(maskedTree(listOf(MaskShape.All), fadeMs = 350u))

        assertEquals(drawn, channel.fade?.from)
        assertEquals(350_000_000L, channel.fade?.durationNanos)
    }

    @Test
    fun `unknown masks are ignored`() {
        val offsets = RendererGestureOffsets()
        val box = offsets.receive(maskedTree()) as JsWidget.Box
        offsets.receive(RendererNode.ExtendMask("other", listOf(MaskShape.All)))

        assertTrue(requireNotNull(keyedMask(box).channel).added.isEmpty())
    }

    private fun keyedMask(box: JsWidget.Box): JsModifier.Mask =
        (box.children[1] as JsWidget.Image).modifiers.filterIsInstance<JsModifier.Mask>().single()

    private fun maskedTree(shapes: List<MaskShape> = emptyList(), fadeMs: UShort? = null) = RendererNode.Box(
        modifiers = emptyList(),
        props = BoxProps(contentAlignment = null),
        children = listOf(
            gesture("pan", location = true),
            RendererNode.Image(
                modifiers = listOf(
                    Modifier.Mask(Mask("mask", MaskMode.HIDE, shapes, MaskFollow("pan", 5200u), fadeMs, null)),
                ),
                props = ImageProps(source = ImageSource.Archive("picture.png"), fit = null),
            ),
        ),
    )

    private fun gesture(key: String, location: Boolean = false) = RendererNode.Gesture(
        modifiers = emptyList(),
        props = GestureProps(
            tapAction = null,
            pan = if (location) {
                PanGesture(PanAxis.FREE, null, null, "move", null, null, true)
            } else {
                null
            },
            accessibilityLabel = null,
            enabled = null,
            hapticOnComplete = null,
            key = key,
            feel = null,
        ),
        children = emptyList(),
    )
}
