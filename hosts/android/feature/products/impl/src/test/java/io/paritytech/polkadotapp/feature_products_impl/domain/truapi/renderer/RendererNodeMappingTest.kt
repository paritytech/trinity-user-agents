package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.Background
import uniffi.truapi.BorderStyle
import uniffi.truapi.ButtonProps
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ColumnProps
import uniffi.truapi.Dimensions
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.Modifier
import uniffi.truapi.RendererNode
import uniffi.truapi.Shape
import uniffi.truapi.TextProps

class RendererNodeMappingTest {
    @Test
    fun `a streamed tree maps like a decoded preview, so both faces draw alike`() {
        val node = RendererNode.Column(
            modifiers = listOf(
                Modifier.Padding(Dimensions(top = 12u, end = 8u, bottom = null, start = 4u)),
                Modifier.Background(Background(ColorToken.BG_SURFACE_NESTED, Shape.Circle)),
                Modifier.FillWidth(true),
                Modifier.FillHeight(false),
                Modifier.Width(40u),
                Modifier.MinHeight(10u),
            ),
            props = ColumnProps(horizontalAlignment = HorizontalAlignment.CENTER, verticalArrangement = null),
            children = listOf(
                RendererNode.Nil,
                RendererNode.Text(
                    modifiers = emptyList(),
                    props = TextProps(style = null, color = ColorToken.FG_WARNING),
                    children = listOf(RendererNode.String("Votes: "), RendererNode.String("1")),
                ),
                RendererNode.Button(
                    modifiers = emptyList(),
                    props = ButtonProps(text = "Vote", variant = ButtonVariant.SECONDARY, enabled = null, loading = true, clickAction = "vote"),
                    children = emptyList(),
                ),
            ),
        )

        val widget = node.toJsWidget() as JsWidget.Column

        assertEquals(JsHorizontalAlignment.CENTER, widget.horizontalAlignment)
        assertEquals(
            listOf(
                JsModifier.Padding(top = 12, end = 8, bottom = 12, start = 4),
                JsModifier.Background(JsColor.BG_SURFACE_NESTED, JsShape.Circle),
                JsModifier.FillMaxWidth(),
                JsModifier.Size(width = 40, height = null, minWidth = null, minHeight = 10),
            ),
            widget.modifiers,
        )
        assertEquals(2, widget.children.size)

        val text = widget.children[0] as JsWidget.Text
        assertEquals("Votes: 1", text.text)
        assertEquals(JsColor.FG_WARNING, text.color)

        val button = widget.children[1] as JsWidget.Button
        assertEquals(JsButtonVariant.SECONDARY, button.variant)
        assertEquals("absent enabled defaults to true", true, button.enabled)
        assertEquals(true, button.loading)
        assertEquals("vote", button.onClick)
    }

    // The core carries a size as an unsigned 64-bit number and Compose draws in Int dp, so a bare
    // conversion turns 4294967295 into -1 and Modifier.padding throws while the card composes. A
    // product must not be able to take the Pocket tab down with a number.
    @Test
    fun `a size no screen could hold is clamped rather than wrapped into a negative one`() {
        val node = RendererNode.Spacer(
            modifiers = listOf(
                Modifier.Padding(Dimensions(top = 4294967295u, end = 0u, bottom = null, start = null)),
                Modifier.Width(ULong.MAX_VALUE),
                Modifier.Border(BorderStyle(width = ULong.MAX_VALUE, color = ColorToken.FG_PRIMARY, shape = null)),
            ),
        )

        val modifiers = (node.toJsWidget() as JsWidget.Spacer).modifiers

        val padding = modifiers.filterIsInstance<JsModifier.Padding>().single()
        assertTrue("padding stayed drawable", listOfNotNull(padding.top, padding.end, padding.bottom, padding.start).all { it >= 0 })
        assertTrue("width stayed drawable", modifiers.filterIsInstance<JsModifier.Size>().single().width!! >= 0)
        assertTrue("border stayed drawable", modifiers.filterIsInstance<JsModifier.Border>().single().width >= 0)
    }

    @Test
    fun `a size within reach is carried through unchanged`() {
        val node = RendererNode.Spacer(modifiers = listOf(Modifier.Width(40u), Modifier.MinHeight(10u)))

        val size = (node.toJsWidget() as JsWidget.Spacer).modifiers.filterIsInstance<JsModifier.Size>().single()

        assertEquals(40, size.width)
        assertEquals(10, size.minHeight)
    }
}
