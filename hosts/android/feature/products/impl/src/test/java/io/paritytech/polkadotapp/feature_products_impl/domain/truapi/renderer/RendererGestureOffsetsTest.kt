package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.truapi.BoxProps
import uniffi.truapi.GestureProps
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

    private fun gesture(key: String) = RendererNode.Gesture(
        modifiers = emptyList(),
        props = GestureProps(
            tapAction = null,
            pan = null,
            accessibilityLabel = null,
            enabled = null,
            hapticOnComplete = null,
            key = key,
        ),
        children = emptyList(),
    )
}
