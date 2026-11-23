package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanDrag
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTravel
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.Background
import uniffi.truapi.BorderStyle
import uniffi.truapi.BoxProps
import uniffi.truapi.ButtonProps
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ColumnProps
import uniffi.truapi.Dimensions
import uniffi.truapi.DropShadow
import uniffi.truapi.Fill
import uniffi.truapi.GestureProps
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.Modifier
import uniffi.truapi.PanAxis
import uniffi.truapi.PanDrag
import uniffi.truapi.PanFade
import uniffi.truapi.PanFeel
import uniffi.truapi.PanGesture
import uniffi.truapi.PanRelease
import uniffi.truapi.PanTravel
import uniffi.truapi.RendererNode
import uniffi.truapi.Shape
import uniffi.truapi.SlideToConfirmProps
import uniffi.truapi.SlideToConfirmVariant
import uniffi.truapi.TextFont
import uniffi.truapi.TextProps
import uniffi.truapi.Translate

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
    fun `a gesture with a host drag carries its children, travel and release`() {
        val node = RendererNode.Gesture(
            modifiers = listOf(Modifier.FillWidth(true)),
            props = GestureProps(
                key = null,
                tapAction = null,
                pan = PanGesture(
                    axis = PanAxis.HORIZONTAL,
                    drag = PanDrag(travel = PanTravel.ParentWidth, release = PanRelease.Complete(90u), holdAtEnd = null),
                    startAction = "start",
                    moveAction = null,
                    moveIntervalMs = null,
                    endAction = "end",
                ),
                accessibilityLabel = "Slide to register",
                enabled = null,
                hapticOnComplete = true,
                feel = null,
            ),
            children = listOf(RendererNode.Box(modifiers = emptyList(), props = BoxProps(contentAlignment = null), children = emptyList())),
        )

        val widget = node.toJsWidget() as JsWidget.Gesture

        assertEquals(
            JsWidget.Gesture(
                pan = JsPanGesture(
                    axis = JsPanAxis.HORIZONTAL,
                    drag = JsPanDrag(travel = JsPanTravel.ParentWidth, release = JsPanRelease.Complete(90), holdAtEnd = true),
                    startAction = "start",
                    endAction = "end",
                ),
                accessibilityLabel = "Slide to register",
                enabled = true,
                hapticOnComplete = true,
                modifiers = listOf(JsModifier.FillMaxWidth()),
                children = listOf(JsWidget.Box()),
            ),
            widget,
        )
    }

    @Test
    fun `a gesture without a drag reports its moves at the interval it asked for`() {
        val node = RendererNode.Gesture(
            modifiers = emptyList(),
            props = GestureProps(
                key = null,
                tapAction = "tap",
                pan = PanGesture(
                    axis = PanAxis.FREE,
                    drag = null,
                    startAction = null,
                    moveAction = "move",
                    moveIntervalMs = 16u,
                    endAction = null,
                ),
                accessibilityLabel = null,
                enabled = false,
                hapticOnComplete = null,
                feel = null,
            ),
            children = emptyList(),
        )

        val widget = node.toJsWidget() as JsWidget.Gesture

        assertEquals(JsPanGesture(axis = JsPanAxis.FREE, moveAction = "move", moveIntervalMs = 16), widget.pan)
        assertEquals("tap", widget.tapAction)
        assertFalse(widget.enabled)
        assertFalse(widget.hapticOnComplete)
    }

    @Test
    fun `a slide to confirm carries every field the product set`() {
        val node = RendererNode.SlideToConfirm(
            modifiers = listOf(Modifier.Width(200u)),
            props = SlideToConfirmProps(
                text = "Slide to register",
                secondaryText = "Fri 14 Nov",
                confirmedText = "Registered",
                variant = SlideToConfirmVariant.REVEAL,
                accent = ColorToken.FG_SUCCESS,
                enabled = false,
                loading = true,
                confirmed = true,
                confirmAction = "register",
            ),
        )

        val widget = node.toJsWidget() as JsWidget.SlideToConfirm

        assertEquals(
            JsWidget.SlideToConfirm(
                text = "Slide to register",
                secondaryText = "Fri 14 Nov",
                confirmedText = "Registered",
                variant = JsSlideToConfirmVariant.REVEAL,
                accent = JsColor.FG_SUCCESS,
                onConfirm = "register",
                enabled = false,
                loading = true,
                confirmed = true,
                modifiers = listOf(JsModifier.Size(width = 200, height = null, minWidth = null, minHeight = null)),
            ),
            widget,
        )
    }

    // An absent OptionBool leaves the default to the host: a thumb that accepts a slide.
    @Test
    fun `a slide to confirm with only a label is a ready thumb`() {
        val node = RendererNode.SlideToConfirm(
            modifiers = emptyList(),
            props = SlideToConfirmProps(
                text = "Slide to register",
                secondaryText = null,
                confirmedText = null,
                variant = null,
                accent = null,
                enabled = null,
                loading = null,
                confirmed = null,
                confirmAction = null,
            ),
        )

        val widget = node.toJsWidget() as JsWidget.SlideToConfirm

        assertEquals(JsSlideToConfirmVariant.THUMB, widget.variant)
        assertEquals(true, widget.enabled)
        assertEquals(false, widget.loading)
        assertEquals(false, widget.confirmed)
        assertEquals(null, widget.accent)
        assertEquals(null, widget.onConfirm)
    }

    @Test
    fun `a size within reach is carried through unchanged`() {
        val node = RendererNode.Spacer(modifiers = listOf(Modifier.Width(40u), Modifier.MinHeight(10u)))

        val size = (node.toJsWidget() as JsWidget.Spacer).modifiers.filterIsInstance<JsModifier.Size>().single()

        assertEquals(40, size.width)
        assertEquals(10, size.minHeight)
    }

    @Test
    fun `modifiers by value carry the existing slider's values in dp, colours as 0xRRGGBBAA`() {
        val node = RendererNode.Box(
            modifiers = listOf(
                Modifier.Fill(Fill(color = 0x33363AFFu, shape = Shape.Circle)),
                Modifier.Shadow(DropShadow(color = 0x0000005Cu, x = 0, y = 200, blur = 600u)),
                Modifier.Translate(Translate(x = 0, y = -180)),
                Modifier.Font(TextFont(size = 1800u, lineHeight = 2200u, weight = 600u, color = 0xF5F6F9FFu, letterSpacing = 0)),
                Modifier.FadeWithPan(PanFade(key = "thumb", ratePermille = 1400u)),
                Modifier.Spin(800u),
                Modifier.ExitFade(350u),
            ),
            props = BoxProps(contentAlignment = null),
            children = emptyList(),
        )

        val widget = node.toJsWidget() as JsWidget.Box

        assertEquals(
            listOf(
                JsModifier.Fill(0x33363AFFL, JsShape.Circle),
                JsModifier.Shadow(0x0000005CL, 0f, 2f, 6f),
                JsModifier.Translate(0f, -1.8f),
                JsModifier.Font(size = 18f, lineHeight = 22f, weight = 600, rgba = 0xF5F6F9FFL, letterSpacing = 0f),
                JsModifier.FadeWithPan("thumb", 1.4f),
                JsModifier.Spin(800),
                JsModifier.ExitFade(350),
            ),
            widget.modifiers,
        )
    }

    @Test
    fun `a gesture's feel and key reach the widget in its own units`() {
        val node = RendererNode.Gesture(
            modifiers = emptyList(),
            props = GestureProps(
                key = "thumb",
                tapAction = null,
                pan = null,
                accessibilityLabel = null,
                enabled = null,
                hapticOnComplete = null,
                feel = PanFeel(
                    pressScalePermille = 1059u,
                    pressMs = 133u,
                    unpressMs = 150u,
                    springOmegaCenti = 1800u,
                    springDampingCenti = 80u,
                    approachMs = 117u,
                    approachReference = 4200u,
                    popPermille = listOf(1059u, 1237u, 1000u),
                    popOffset = 333u,
                ),
            ),
            children = emptyList(),
        )

        val widget = node.toJsWidget() as JsWidget.Gesture

        assertEquals("thumb", widget.key)
        assertEquals(
            JsPanFeel(
                pressScale = 1.059f,
                pressMs = 133,
                unpressMs = 150,
                springOmega = 18f,
                springDamping = 0.8f,
                approachMs = 117,
                approachReference = 42f,
                pop = listOf(1.059f, 1.237f, 1f),
                popOffset = 3.33f,
            ),
            widget.feel,
        )
    }
}
