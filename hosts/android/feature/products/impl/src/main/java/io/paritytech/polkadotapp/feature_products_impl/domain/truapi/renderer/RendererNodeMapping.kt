package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer

import io.paritytech.polkadotapp.feature_products_api.model.JsAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsArrangement
import io.paritytech.polkadotapp.feature_products_api.model.JsBlendingMode
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsImageFit
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
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
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsVerticalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.toJsImageSource
import uniffi.truapi.Arrangement
import uniffi.truapi.BlendingMode
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ContentAlignment
import uniffi.truapi.Dimensions
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.ImageFit
import uniffi.truapi.Mask
import uniffi.truapi.MaskBrush
import uniffi.truapi.MaskBrushTip
import uniffi.truapi.MaskMode
import uniffi.truapi.MaskPoint
import uniffi.truapi.MaskShape
import uniffi.truapi.Modifier
import uniffi.truapi.PanAxis
import uniffi.truapi.PanDrag
import uniffi.truapi.PanFeel
import uniffi.truapi.PanGesture
import uniffi.truapi.PanRelease
import uniffi.truapi.PanTravel
import uniffi.truapi.RendererNode
import uniffi.truapi.Shape
import uniffi.truapi.Size
import uniffi.truapi.TypographyStyle
import uniffi.truapi.VerticalAlignment

/**
 * What a tree's keyed nodes share with the updates between trees, by key: a gesture's offset, a location pan's
 * strokes, and a mask's added shapes. None by default, for a tree drawn once.
 */
class RendererKeyed(
    val gesturePosition: (String) -> JsGesturePosition? = { null },
    val panTrail: (String) -> JsPanTrail? = { null },
    val maskShapes: (String) -> JsMaskShapes? = { null },
)

/**
 * Maps a core renderer tree onto the widget model the Compose interpreter draws. `Nil` draws
 * nothing and a `String` child is absorbed as its parent's text, as the chat mapper does.
 */
fun RendererNode.toJsWidget(keyed: RendererKeyed = RendererKeyed()): JsWidget =
    JsWidgetMapping(keyed).map(this)

fun RendererNode.hasMaterials(): Boolean = materialsCount() > 0

private fun RendererNode.materialsCount(): Int = when (this) {
    is RendererNode.Materials -> 1 + children.sumOf { it.materialsCount() }
    is RendererNode.Box -> children.sumOf { it.materialsCount() }
    is RendererNode.Column -> children.sumOf { it.materialsCount() }
    is RendererNode.Row -> children.sumOf { it.materialsCount() }
    is RendererNode.Text -> children.sumOf { it.materialsCount() }
    is RendererNode.Button -> children.sumOf { it.materialsCount() }
    is RendererNode.Gesture -> children.sumOf { it.materialsCount() }
    is RendererNode.Nil,
    is RendererNode.SetOffset,
    is RendererNode.ExtendMask,
    is RendererNode.String,
    is RendererNode.Spacer,
    is RendererNode.TextField,
    is RendererNode.Image -> 0
}

private class JsWidgetMapping(private val keyed: RendererKeyed) {
    private var nextMaterialsNode = 0

    fun map(node: RendererNode): JsWidget = when (node) {
        is RendererNode.Nil, is RendererNode.SetOffset, is RendererNode.ExtendMask -> JsWidget.Spacer()
        is RendererNode.String -> JsWidget.Text(text = node.text)
        is RendererNode.Box -> JsWidget.Box(
            modifiers = node.modifiers.toJsModifiers(keyed),
            children = mapChildren(node.children),
            contentAlignment = node.props.contentAlignment?.toJs(),
        )
        is RendererNode.Column -> JsWidget.Column(
            modifiers = node.modifiers.toJsModifiers(keyed),
            children = mapChildren(node.children),
            horizontalAlignment = node.props.horizontalAlignment?.toJs(),
            verticalArrangement = node.props.verticalArrangement?.toJs(),
        )
        is RendererNode.Row -> JsWidget.Row(
            modifiers = node.modifiers.toJsModifiers(keyed),
            children = mapChildren(node.children),
            verticalAlignment = node.props.verticalAlignment?.toJs(),
            horizontalArrangement = node.props.horizontalArrangement?.toJs(),
        )
        is RendererNode.Spacer -> JsWidget.Spacer(modifiers = node.modifiers.toJsModifiers(keyed))
        is RendererNode.Text -> {
            skipMaterialsIn(node.children)
            JsWidget.Text(
                text = node.children.filterIsInstance<RendererNode.String>().joinToString("") { it.text },
                style = node.props.style?.toJs() ?: JsTypographyStyle.BODY_LARGE_REGULAR,
                color = node.props.color?.toJs(),
                modifiers = node.modifiers.toJsModifiers(keyed),
            )
        }
        is RendererNode.Button -> {
            skipMaterialsIn(node.children)
            JsWidget.Button(
                text = node.props.text,
                onClick = node.props.clickAction,
                variant = node.props.variant?.toJs() ?: JsButtonVariant.PRIMARY,
                enabled = node.props.enabled ?: true,
                loading = node.props.loading ?: false,
                modifiers = node.modifiers.toJsModifiers(keyed),
            )
        }
        is RendererNode.TextField -> JsWidget.TextField(
            value = node.props.text,
            onValueChange = node.props.valueChangeAction,
            placeholder = node.props.placeholder,
            label = node.props.label,
            enabled = node.props.enabled ?: true,
            modifiers = node.modifiers.toJsModifiers(keyed),
        )
        // An image this host cannot draw keeps the size its modifiers give it.
        is RendererNode.Image -> node.props.source.toJsImageSource()?.let { source ->
            JsWidget.Image(
                source = source,
                fit = node.props.fit?.toJs() ?: JsImageFit.FILL,
                modifiers = node.modifiers.toJsModifiers(keyed),
            )
        } ?: JsWidget.Box(modifiers = node.modifiers.toJsModifiers(keyed))
        is RendererNode.Gesture -> JsWidget.Gesture(
            position = node.props.key?.let(keyed.gesturePosition),
            trail = node.props.key?.takeIf { node.props.pan?.location == true }?.let(keyed.panTrail),
            tapAction = node.props.tapAction,
            pan = node.props.pan?.toJs(),
            accessibilityLabel = node.props.accessibilityLabel,
            enabled = node.props.enabled ?: true,
            hapticOnComplete = node.props.hapticOnComplete ?: false,
            modifiers = node.modifiers.toJsModifiers(keyed),
            children = mapChildren(node.children),
            key = node.props.key,
            feel = node.props.feel?.toJs(),
        )
        is RendererNode.Materials -> {
            val index = nextMaterialsNode++
            JsWidget.Materials(node = index, children = mapChildren(node.children))
        }
    }

    private fun mapChildren(children: List<RendererNode>): List<JsWidget> =
        children.filterNot {
            it is RendererNode.Nil || it is RendererNode.String || it is RendererNode.SetOffset || it is RendererNode.ExtendMask
        }.map(::map)

    // The core numbers `Materials` nodes under a `Text` or `Button` although their children are not drawn.
    private fun skipMaterialsIn(children: List<RendererNode>) {
        nextMaterialsNode += children.sumOf { it.materialsCount() }
    }
}

// The Android mask-progress layer scales each axis apart; until then `pressScaleYPermille` is ignored.
private fun PanFeel.toJs(): JsPanFeel = JsPanFeel(
    pressScale = pressScalePermille.toFloat() / PERMILLE,
    pressMs = pressMs.toInt(),
    unpressMs = unpressMs.toInt(),
    springOmega = springOmegaCenti.toFloat() / HUNDREDTHS,
    springDamping = springDampingCenti.toFloat() / HUNDREDTHS,
    approachMs = approachMs.toInt(),
    approachReference = approachReference.toFloat() / HUNDREDTHS,
    pop = popPermille.map { it.toFloat() / PERMILLE },
    popOffset = popOffset.toFloat() / HUNDREDTHS,
)

private fun PanGesture.toJs(): JsPanGesture = JsPanGesture(
    axis = axis.toJs(),
    drag = drag?.toJs(),
    startAction = startAction,
    moveAction = moveAction,
    moveIntervalMs = moveIntervalMs?.toInt() ?: 0,
    endAction = endAction,
    location = location ?: false,
)

private fun PanAxis.toJs(): JsPanAxis = when (this) {
    PanAxis.HORIZONTAL -> JsPanAxis.HORIZONTAL
    PanAxis.VERTICAL -> JsPanAxis.VERTICAL
    PanAxis.FREE -> JsPanAxis.FREE
}

// A completed drag stays at the end unless the product says otherwise.
private fun PanDrag.toJs(): JsPanDrag = JsPanDrag(
    travel = when (val travel = travel) {
        is PanTravel.ParentWidth -> JsPanTravel.ParentWidth
        is PanTravel.Points -> JsPanTravel.Points(travel.v1.toDp())
    },
    release = when (val release = release) {
        is PanRelease.SnapBack -> JsPanRelease.SnapBack
        is PanRelease.Stay -> JsPanRelease.Stay
        is PanRelease.Complete -> JsPanRelease.Complete(release.thresholdPercent.toInt())
    },
    holdAtEnd = holdAtEnd ?: true,
)

private fun List<Modifier>.toJsModifiers(keyed: RendererKeyed = RendererKeyed()): List<JsModifier> = buildList {
    var width: Int? = null
    var height: Int? = null
    var minWidth: Int? = null
    var minHeight: Int? = null

    for (modifier in this@toJsModifiers) {
        when (modifier) {
            is Modifier.Margin -> add(modifier.v1.toMargin())
            is Modifier.Padding -> add(modifier.v1.toPadding())
            is Modifier.Background -> add(JsModifier.Background(modifier.v1.color.toJs(), modifier.v1.shape?.toJs()))
            is Modifier.Border -> add(
                JsModifier.Border(modifier.v1.width.toDp(), modifier.v1.color.toJs(), modifier.v1.shape?.toJs()),
            )
            is Modifier.Height -> height = modifier.v1.toDp()
            is Modifier.Width -> width = modifier.v1.toDp()
            is Modifier.MinWidth -> minWidth = modifier.v1.toDp()
            is Modifier.MinHeight -> minHeight = modifier.v1.toDp()
            is Modifier.FillWidth -> if (modifier.v1) add(JsModifier.FillMaxWidth())
            is Modifier.FillHeight -> if (modifier.v1) add(JsModifier.FillMaxHeight())
            is Modifier.Opacity -> add(JsModifier.Opacity(modifier.v1.toInt()))
            is Modifier.BlendingMode -> add(JsModifier.BlendingMode(modifier.v1.toJs()))
            is Modifier.Fill -> add(JsModifier.Fill(modifier.v1.color.toLong(), modifier.v1.shape?.toJs()))
            is Modifier.Shadow -> with(modifier.v1) {
                add(JsModifier.Shadow(color.toLong(), x.hundredths(), y.hundredths(), blur.hundredths()))
            }
            is Modifier.Translate -> add(JsModifier.Translate(modifier.v1.x.hundredths(), modifier.v1.y.hundredths()))
            is Modifier.Font -> with(modifier.v1) {
                add(
                    JsModifier.Font(
                        size = size.hundredths(),
                        lineHeight = lineHeight.hundredths(),
                        weight = weight.toInt(),
                        rgba = color.toLong(),
                        letterSpacing = letterSpacing.hundredths(),
                    ),
                )
            }
            is Modifier.FadeWithPan -> add(JsModifier.FadeWithPan(modifier.v1.key, modifier.v1.ratePermille.toFloat() / PERMILLE))
            is Modifier.Shimmer -> with(modifier.v1) {
                add(
                    JsModifier.Shimmer(
                        rgba = color.toLong(),
                        stops = stops.map { (it.toInt() and BYTE_MASK) / BYTE_MAX },
                        band = band.hundredths(),
                        lead = lead.hundredths(),
                        speed = speed.hundredths(),
                        restMs = restMs.toInt(),
                    ),
                )
            }
            is Modifier.Spin -> add(JsModifier.Spin(modifier.v1.toInt()))
            is Modifier.ExitFade -> add(JsModifier.ExitFade(modifier.v1.toInt()))
            is Modifier.Mask -> add(modifier.v1.toJs(keyed))
            // The Android enter-and-keyframes layer plays these; until then the node draws as the tree does.
            is Modifier.Enter, is Modifier.Animate -> Unit
            // The Android particle layer draws emitters; until then the node draws with no particles.
            is Modifier.Emit -> Unit
            // The Android parallax layer moves the node by tilt and pan; until then it stays where the tree puts it.
            is Modifier.Parallax -> Unit
        }
    }

    if (width != null || height != null || minWidth != null || minHeight != null) {
        add(JsModifier.Size(width = width, height = height, minWidth = minWidth, minHeight = minHeight))
    }
}

/**
 * The core carries a size as an unsigned 64-bit number and Compose draws in Int dp, so a bare
 * conversion turns 4294967295 into -1, which throws the moment the padding is applied. A product
 * must not be able to take the card's screen down with a number, so a size beyond anything a screen
 * could hold is drawn at that bound instead of rejected: the card keeps drawing, visibly wrong.
 */
private fun Size.toDp(): Int = coerceAtMost(MAX_DRAWABLE_DP).toInt()

// Lengths by value travel in hundredths of a point.
private fun UInt.hundredths(): Float = toFloat() / HUNDREDTHS
private fun Int.hundredths(): Float = toFloat() / HUNDREDTHS

private const val HUNDREDTHS = 100f
private const val PERMILLE = 1000f
private const val BYTE_MASK = 0xFF
private const val BYTE_MAX = 255f

// Two orders of magnitude past the longest edge of any device, and far from where dp-to-pixel
// arithmetic overflows.
private const val MAX_DRAWABLE_DP: Size = 100_000uL

// `bottom` defaults to `top` and `start` to `end` when absent.
private fun Dimensions.toMargin() = JsModifier.Margin(
    top = top.toDp(),
    end = end.toDp(),
    bottom = (bottom ?: top).toDp(),
    start = (start ?: end).toDp(),
)

private fun Dimensions.toPadding() = JsModifier.Padding(
    top = top.toDp(),
    end = end.toDp(),
    bottom = (bottom ?: top).toDp(),
    start = (start ?: end).toDp(),
)

private fun Mask.toJs(keyed: RendererKeyed): JsModifier.Mask = JsModifier.Mask(
    key = key,
    mode = if (mode == MaskMode.SHOW) JsMaskMode.SHOW else JsMaskMode.HIDE,
    shapes = shapes.map { it.toJs() },
    follow = follow?.let { JsMaskFollow(it.pan, it.width.toLong(), it.brush?.toJs()) },
    fadeMs = fadeMs?.toLong() ?: 0L,
    // The Android mask-progress layer checks mid-stroke, rescues a stalled share and plays `haptics`; until then
    // `whileDown`, `rescue` and `haptics` are ignored.
    clearedShare = clearedShare?.let { JsClearedShare(it.atPermille.toInt(), it.action) },
    channel = key?.let(keyed.maskShapes),
    trail = follow?.pan?.let(keyed.panTrail),
)

/** A mask's shape, still in the wire's hundredths of a dp. */
fun MaskShape.toJs(): JsMaskShape = when (this) {
    is MaskShape.All -> JsMaskShape.All
    is MaskShape.Circle -> JsMaskShape.Circle(center.toJs(), radius.toLong())
    is MaskShape.Stroke -> JsMaskShape.Stroke(width.toLong(), points.map { it.toJs() })
    is MaskShape.Stamped -> JsMaskShape.Stamped(width.toLong(), brush.toJs(), points.map { it.toJs() })
}

private fun MaskBrush.toJs() = JsMaskBrush(
    tip = if (tip == MaskBrushTip.CAPSULE) JsMaskBrushTip.CAPSULE else JsMaskBrushTip.ELLIPSE,
    aspectPermille = aspectPermille.toInt(),
    angleCentideg = angleCentideg.toInt(),
    spacingPermille = spacingPermille.toInt(),
)

private fun MaskPoint.toJs() = JsMaskPoint(x, y)

private fun Shape.toJs(): JsShape = when (this) {
    is Shape.Rounded -> JsShape.Rounded(radius = v1.toDp())
    is Shape.Circle -> JsShape.Circle
    is Shape.Square -> JsShape.Square
}

private fun ImageFit.toJs(): JsImageFit = when (this) {
    ImageFit.NONE -> JsImageFit.NONE
    ImageFit.FILL -> JsImageFit.FILL
    ImageFit.COVER -> JsImageFit.COVER
    ImageFit.CONTAIN -> JsImageFit.CONTAIN
    ImageFit.SCALE_DOWN -> JsImageFit.SCALE_DOWN
}

private fun BlendingMode.toJs(): JsBlendingMode = when (this) {
    BlendingMode.NORMAL -> JsBlendingMode.NORMAL
    BlendingMode.MULTIPLY -> JsBlendingMode.MULTIPLY
    BlendingMode.SCREEN -> JsBlendingMode.SCREEN
    BlendingMode.OVERLAY -> JsBlendingMode.OVERLAY
    BlendingMode.DARKEN -> JsBlendingMode.DARKEN
    BlendingMode.LIGHTEN -> JsBlendingMode.LIGHTEN
    BlendingMode.COLOR_DODGE -> JsBlendingMode.COLOR_DODGE
    BlendingMode.COLOR_BURN -> JsBlendingMode.COLOR_BURN
    BlendingMode.HARD_LIGHT -> JsBlendingMode.HARD_LIGHT
    BlendingMode.SOFT_LIGHT -> JsBlendingMode.SOFT_LIGHT
    BlendingMode.DIFFERENCE -> JsBlendingMode.DIFFERENCE
    BlendingMode.EXCLUSION -> JsBlendingMode.EXCLUSION
    BlendingMode.HUE -> JsBlendingMode.HUE
    BlendingMode.SATURATION -> JsBlendingMode.SATURATION
    BlendingMode.COLOR -> JsBlendingMode.COLOR
    BlendingMode.LUMINOSITY -> JsBlendingMode.LUMINOSITY
}

private fun ColorToken.toJs(): JsColor = when (this) {
    ColorToken.FG_PRIMARY -> JsColor.FG_PRIMARY
    ColorToken.FG_SECONDARY -> JsColor.FG_SECONDARY
    ColorToken.FG_TERTIARY -> JsColor.FG_TERTIARY
    ColorToken.BG_SURFACE_MAIN -> JsColor.BG_SURFACE_MAIN
    ColorToken.BG_SURFACE_CONTAINER -> JsColor.BG_SURFACE_CONTAINER
    ColorToken.BG_SURFACE_NESTED -> JsColor.BG_SURFACE_NESTED
    ColorToken.FG_SUCCESS -> JsColor.FG_SUCCESS
    ColorToken.FG_ERROR -> JsColor.FG_ERROR
    ColorToken.FG_WARNING -> JsColor.FG_WARNING
}

private fun TypographyStyle.toJs(): JsTypographyStyle = when (this) {
    TypographyStyle.HEADLINE_LARGE -> JsTypographyStyle.HEADLINE_LARGE
    TypographyStyle.TITLE_MEDIUM_REGULAR -> JsTypographyStyle.TITLE_MEDIUM_REGULAR
    TypographyStyle.BODY_LARGE_REGULAR -> JsTypographyStyle.BODY_LARGE_REGULAR
    TypographyStyle.BODY_MEDIUM_REGULAR -> JsTypographyStyle.BODY_MEDIUM_REGULAR
    TypographyStyle.BODY_SMALL_REGULAR -> JsTypographyStyle.BODY_SMALL_REGULAR
}

private fun ButtonVariant.toJs(): JsButtonVariant = when (this) {
    ButtonVariant.PRIMARY -> JsButtonVariant.PRIMARY
    ButtonVariant.SECONDARY -> JsButtonVariant.SECONDARY
    ButtonVariant.TEXT -> JsButtonVariant.TEXT
}

private fun ContentAlignment.toJs(): JsAlignment = when (this) {
    ContentAlignment.TOP_START -> JsAlignment.TOP_START
    ContentAlignment.TOP_CENTER -> JsAlignment.TOP_CENTER
    ContentAlignment.TOP_END -> JsAlignment.TOP_END
    ContentAlignment.CENTER_START -> JsAlignment.CENTER_START
    ContentAlignment.CENTER -> JsAlignment.CENTER
    ContentAlignment.CENTER_END -> JsAlignment.CENTER_END
    ContentAlignment.BOTTOM_START -> JsAlignment.BOTTOM_START
    ContentAlignment.BOTTOM_CENTER -> JsAlignment.BOTTOM_CENTER
    ContentAlignment.BOTTOM_END -> JsAlignment.BOTTOM_END
}

private fun HorizontalAlignment.toJs(): JsHorizontalAlignment = when (this) {
    HorizontalAlignment.START -> JsHorizontalAlignment.START
    HorizontalAlignment.CENTER -> JsHorizontalAlignment.CENTER
    HorizontalAlignment.END -> JsHorizontalAlignment.END
}

private fun VerticalAlignment.toJs(): JsVerticalAlignment = when (this) {
    VerticalAlignment.TOP -> JsVerticalAlignment.TOP
    VerticalAlignment.CENTER -> JsVerticalAlignment.CENTER
    VerticalAlignment.BOTTOM -> JsVerticalAlignment.BOTTOM
}

private fun Arrangement.toJs(): JsArrangement = when (this) {
    Arrangement.START -> JsArrangement.START
    Arrangement.END -> JsArrangement.END
    Arrangement.CENTER -> JsArrangement.CENTER
    Arrangement.SPACE_BETWEEN -> JsArrangement.SPACE_BETWEEN
    Arrangement.SPACE_AROUND -> JsArrangement.SPACE_AROUND
    Arrangement.SPACE_EVENLY -> JsArrangement.SPACE_EVENLY
}
