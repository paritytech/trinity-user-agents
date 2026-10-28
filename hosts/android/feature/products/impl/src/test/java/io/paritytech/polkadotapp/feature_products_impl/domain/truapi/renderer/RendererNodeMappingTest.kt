package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsArrangement
import io.paritytech.polkadotapp.feature_products_api.model.JsBlendingMode
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsEffect
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsImageFit
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.Arrangement
import uniffi.truapi.Background
import uniffi.truapi.BlendingMode
import uniffi.truapi.BorderStyle
import uniffi.truapi.BoxProps
import uniffi.truapi.ButtonProps
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ColumnProps
import uniffi.truapi.ContentAlignment
import uniffi.truapi.Dimensions
import uniffi.truapi.Effect
import uniffi.truapi.EffectProps
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.ImageFit
import uniffi.truapi.ImageProps
import uniffi.truapi.ImageSource
import uniffi.truapi.Modifier
import uniffi.truapi.RendererNode
import uniffi.truapi.Shape
import uniffi.truapi.TextProps
import uniffi.truapi.TypographyStyle

class RendererNodeMappingTest {
    @Test
    fun `a renderer tree maps onto the native widget vocabulary`() {
        val node = RendererNode.Column(
            modifiers = listOf(
                Modifier.Padding(Dimensions(top = 12u, end = 8u, bottom = null, start = 4u)),
                Modifier.Background(Background(ColorToken.BG_SURFACE_NESTED, Shape.Circle)),
                Modifier.FillWidth(true),
                Modifier.FillHeight(false),
                Modifier.Width(40u),
                Modifier.MinHeight(10u),
            ),
            props = ColumnProps(horizontalAlignment = HorizontalAlignment.CENTER, verticalArrangement = Arrangement.SPACE_BETWEEN),
            children = listOf(
                RendererNode.Nil,
                RendererNode.Text(
                    modifiers = emptyList(),
                    props = TextProps(style = TypographyStyle.HEADLINE_LARGE, color = ColorToken.FG_WARNING),
                    children = listOf(RendererNode.String("Votes: "), RendererNode.String("1")),
                ),
                RendererNode.Button(
                    modifiers = emptyList(),
                    props = ButtonProps(text = "Vote", variant = ButtonVariant.SECONDARY, enabled = null, loading = true, clickAction = "vote"),
                    children = emptyList(),
                ),
                RendererNode.Box(
                    modifiers = emptyList(),
                    props = BoxProps(contentAlignment = ContentAlignment.CENTER_END),
                    children = listOf(RendererNode.Nil),
                ),
                RendererNode.Spacer(modifiers = listOf(Modifier.Opacity(128u))),
            ),
        )

        val widget = node.toJsWidget() as JsWidget.Column

        assertEquals(JsHorizontalAlignment.CENTER, widget.horizontalAlignment)
        assertEquals(JsArrangement.SPACE_BETWEEN, widget.verticalArrangement)
        assertEquals(
            listOf(
                JsModifier.Padding(top = 12, end = 8, bottom = 12, start = 4),
                JsModifier.Background(JsColor.BG_SURFACE_NESTED, JsShape.Circle),
                JsModifier.FillMaxWidth(),
                JsModifier.Size(width = 40, height = null, minWidth = null, minHeight = 10),
            ),
            widget.modifiers,
        )
        assertEquals(4, widget.children.size)

        val text = widget.children[0] as JsWidget.Text
        assertEquals("Votes: 1", text.text)
        assertEquals(JsColor.FG_WARNING, text.color)
        assertEquals(JsTypographyStyle.HEADLINE_LARGE, text.style)

        val button = widget.children[1] as JsWidget.Button
        assertEquals(JsButtonVariant.SECONDARY, button.variant)
        assertEquals("absent enabled defaults to true", true, button.enabled)
        assertEquals(true, button.loading)
        assertEquals("vote", button.onClick)

        assertEquals(JsWidget.Box(contentAlignment = JsAlignment.CENTER_END), widget.children[2])
        assertEquals(JsWidget.Spacer(modifiers = listOf(JsModifier.Opacity(128))), widget.children[3])
    }

    @Test
    fun `effects and images keep their native drawing properties`() {
        val node = RendererNode.Effect(
            props = EffectProps(effect = Effect.RAINBOW),
            children = listOf(
                RendererNode.Image(
                    modifiers = listOf(
                        Modifier.Width(40u),
                        Modifier.BlendingMode(BlendingMode.MULTIPLY),
                        Modifier.Border(BorderStyle(width = 1u, color = ColorToken.FG_TERTIARY, shape = Shape.Square)),
                    ),
                    props = ImageProps(source = ImageSource.Archive("images/stamp.png"), fit = ImageFit.COVER),
                ),
                RendererNode.Image(
                    modifiers = emptyList(),
                    props = ImageProps(source = ImageSource.Bulletin("bafy"), fit = null),
                ),
            ),
        )

        assertEquals(
            JsWidget.Effect(
                effect = JsEffect.RAINBOW,
                children = listOf(
                    JsWidget.Image(
                        source = JsImageSource.Archive("images/stamp.png"),
                        fit = JsImageFit.COVER,
                        modifiers = listOf(
                            JsModifier.BlendingMode(JsBlendingMode.MULTIPLY),
                            JsModifier.Border(1, JsColor.FG_TERTIARY, JsShape.Square),
                            JsModifier.Size(width = 40, height = null, minWidth = null, minHeight = null),
                        ),
                    ),
                    JsWidget.Image(source = JsImageSource.Bulletin("bafy"), fit = JsImageFit.FILL),
                ),
            ),
            node.toJsWidget(),
        )
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
