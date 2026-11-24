package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptic
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptics
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskMode
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanDrag
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTravel
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import io.paritytech.polkadotapp.feature_products_api.model.JsShareRescue
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.Background
import uniffi.truapi.BorderStyle
import uniffi.truapi.BoxProps
import uniffi.truapi.ButtonProps
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ClearedShare
import uniffi.truapi.ColorToken
import uniffi.truapi.ColumnProps
import uniffi.truapi.Dimensions
import uniffi.truapi.DropShadow
import uniffi.truapi.Fill
import uniffi.truapi.GestureProps
import uniffi.truapi.Haptic
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.Mask
import uniffi.truapi.MaskBrush
import uniffi.truapi.MaskBrushTip
import uniffi.truapi.MaskFollow
import uniffi.truapi.MaskHaptics
import uniffi.truapi.MaskMode
import uniffi.truapi.MaskPoint
import uniffi.truapi.MaskShape
import uniffi.truapi.MaterialsProps
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
import uniffi.truapi.ShareRescue
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
                    location = null,
                ),
                accessibilityLabel = "Register",
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
                accessibilityLabel = "Register",
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
                    location = null,
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
    fun `a size within reach is carried through unchanged`() {
        val node = RendererNode.Spacer(modifiers = listOf(Modifier.Width(40u), Modifier.MinHeight(10u)))

        val size = (node.toJsWidget() as JsWidget.Spacer).modifiers.filterIsInstance<JsModifier.Size>().single()

        assertEquals(40, size.width)
        assertEquals(10, size.minHeight)
    }

    // The core settles materials by node number, so a node numbered differently here draws another
    // node's layers. The core counts depth first, parents before children, including nodes inside a Text.
    @Test
    fun `materials nodes take the numbers the core gives them`() {
        fun materials(vararg children: RendererNode) = RendererNode.Materials(MaterialsProps(emptyList()), children.toList())
        val tree = RendererNode.Column(
            modifiers = emptyList(),
            props = ColumnProps(horizontalAlignment = null, verticalArrangement = null),
            children = listOf(
                materials(materials(RendererNode.Spacer(emptyList()))),
                RendererNode.Text(emptyList(), TextProps(style = null, color = null), listOf(RendererNode.String("a"), materials())),
                materials(RendererNode.Spacer(emptyList())),
            ),
        )

        assertEquals(
            JsWidget.Column(
                children = listOf(
                    JsWidget.Materials(0, listOf(JsWidget.Materials(1, listOf(JsWidget.Spacer())))),
                    JsWidget.Text(text = "a"),
                    JsWidget.Materials(3, listOf(JsWidget.Spacer())),
                ),
            ),
            tree.toJsWidget(),
        )
        assertTrue(tree.hasMaterials())
    }

    @Test
    fun `modifiers by value reach the widget in dp, colours as 0xRRGGBBAA`() {
        val node = RendererNode.Box(
            modifiers = listOf(
                Modifier.Fill(Fill(color = 0x33363AFFu, shape = Shape.Circle)),
                Modifier.Shadow(DropShadow(color = 0x0000005Cu, x = 0, y = 200, blur = 600u)),
                Modifier.Translate(Translate(x = 0, y = -180)),
                Modifier.Font(TextFont(size = 1800u, lineHeight = 2200u, weight = 600u, color = 0xF5F6F9FFu, letterSpacing = 0)),
                Modifier.FadeWithPan(PanFade(key = "pan", ratePermille = 1400u)),
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
                JsModifier.FadeWithPan("pan", 1.4f),
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
                key = "pan",
                tapAction = null,
                pan = null,
                accessibilityLabel = null,
                enabled = null,
                hapticOnComplete = null,
                feel = PanFeel(
                    pressScalePermille = 1059u,
                    pressScaleYPermille = 850u,
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

        assertEquals("pan", widget.key)
        assertEquals(
            JsPanFeel(
                pressScale = 1.059f,
                pressScaleY = 0.85f,
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

    @Test
    fun `a mask modifier carries its shapes in hundredths, its follow, fade and keyed state`() {
        val shapes = JsMaskShapes()
        val trail = JsPanTrail()
        val mask = Mask(
            key = "mask",
            mode = MaskMode.HIDE,
            shapes = listOf(
                MaskShape.All,
                MaskShape.Circle(MaskPoint(1050, -200), 2600u),
                MaskShape.Stroke(5200u, listOf(MaskPoint(0, 0), MaskPoint(100, 250))),
            ),
            follow = MaskFollow("pan", 5200u, MaskBrush(MaskBrushTip.ELLIPSE, 400u, 3000, 100u)),
            fadeMs = 350u.toUShort(),
            clearedShare = ClearedShare(atPermille = 550u.toUShort(), action = "cleared", whileDown = true, rescue = ShareRescue(1000u, 400u)),
            haptics = MaskHaptics(100u, Haptic.TICK, Haptic.SUCCESS),
        )
        val node = RendererNode.Box(
            modifiers = listOf(Modifier.Mask(mask)),
            props = BoxProps(contentAlignment = null),
            children = emptyList(),
        )

        val keyed = RendererKeyed(
            panTrail = { key -> trail.takeIf { key == "pan" } },
            maskShapes = { key -> shapes.takeIf { key == "mask" } },
        )
        val mapped = (node.toJsWidget(keyed) as JsWidget.Box).modifiers.single() as JsModifier.Mask

        assertEquals("mask", mapped.key)
        assertEquals(JsMaskMode.HIDE, mapped.mode)
        assertEquals(
            listOf(
                JsMaskShape.All,
                JsMaskShape.Circle(JsMaskPoint(1050, -200), 2600),
                JsMaskShape.Stroke(5200, listOf(JsMaskPoint(0, 0), JsMaskPoint(100, 250))),
            ),
            mapped.shapes,
        )
        assertEquals(JsMaskFollow("pan", 5200, JsMaskBrush(JsMaskBrushTip.ELLIPSE, 400, 3000, 100)), mapped.follow)
        assertEquals(350L, mapped.fadeMs)
        assertEquals(JsClearedShare(550, "cleared", true, JsShareRescue(1000, 400)), mapped.clearedShare)
        assertEquals(JsMaskHaptics(100, JsMaskHaptic.TICK, JsMaskHaptic.SUCCESS), mapped.haptics)
        assertSame(shapes, mapped.channel)
        assertSame(trail, mapped.trail)
    }

    @Test
    fun `an unkeyed rescue gets a new clock when another tree arrives`() {
        val mask = Mask(
            key = null, mode = MaskMode.HIDE, shapes = emptyList(), follow = null, fadeMs = null,
            clearedShare = ClearedShare(900u, "reached", rescue = ShareRescue(1000u, 500u)),
        )
        val node = RendererNode.Spacer(listOf(Modifier.Mask(mask)))
        val first = (node.toJsWidget() as JsWidget.Spacer).modifiers.single() as JsModifier.Mask
        val second = (node.toJsWidget() as JsWidget.Spacer).modifiers.single() as JsModifier.Mask
        assertNotSame(requireNotNull(first.channel), requireNotNull(second.channel))
    }

    @Test
    fun `a show mask with no key keeps no state`() {
        val node = RendererNode.Spacer(
            modifiers = listOf(Modifier.Mask(Mask(key = null, mode = MaskMode.SHOW, shapes = emptyList(), follow = null, fadeMs = null, clearedShare = null))),
        )

        val mapped = (node.toJsWidget() as JsWidget.Spacer).modifiers.single() as JsModifier.Mask

        assertEquals(JsModifier.Mask(mode = JsMaskMode.SHOW), mapped)
    }

    @Test
    fun `mask updates inside a tree draw nothing`() {
        val node = RendererNode.Box(
            modifiers = emptyList(),
            props = BoxProps(contentAlignment = null),
            children = listOf(RendererNode.ExtendMask(key = "mask", shapes = listOf(MaskShape.All))),
        )

        assertEquals(emptyList<JsWidget>(), (node.toJsWidget() as JsWidget.Box).children)
    }

    @Test
    fun `a location pan reports where the finger is`() {
        val node = RendererNode.Gesture(
            modifiers = emptyList(),
            props = GestureProps(
                key = "pan",
                tapAction = null,
                pan = PanGesture(
                    axis = PanAxis.FREE,
                    drag = null,
                    startAction = "start",
                    moveAction = "move",
                    moveIntervalMs = null,
                    endAction = "end",
                    location = true,
                ),
                accessibilityLabel = null,
                enabled = null,
                hapticOnComplete = null,
                feel = null,
            ),
            children = emptyList(),
        )

        val pan = requireNotNull((node.toJsWidget() as JsWidget.Gesture).pan)

        assertTrue(pan.location)
        assertEquals(JsPanAxis.FREE, pan.axis)
    }
}
