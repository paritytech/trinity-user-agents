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
import io.paritytech.polkadotapp.feature_products_api.model.JsVerticalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import uniffi.truapi.Arrangement
import uniffi.truapi.BlendingMode
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ContentAlignment
import uniffi.truapi.Dimensions
import uniffi.truapi.Effect
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.ImageFit
import uniffi.truapi.ImageSource
import uniffi.truapi.Modifier
import uniffi.truapi.RendererNode
import uniffi.truapi.Shape
import uniffi.truapi.Size
import uniffi.truapi.TypographyStyle
import uniffi.truapi.VerticalAlignment

/**
 * Maps a core renderer tree onto the widget model the Compose interpreter draws. `Nil` draws
 * nothing and a `String` child is absorbed as its parent's text, as the chat mapper does.
 */
fun RendererNode.toJsWidget(): JsWidget = when (this) {
    is RendererNode.Nil -> JsWidget.Spacer()
    is RendererNode.String -> JsWidget.Text(text = text)
    is RendererNode.Box -> JsWidget.Box(
        modifiers = modifiers.toJsModifiers(),
        children = children.toJsChildren(),
        contentAlignment = props.contentAlignment?.toJs(),
    )
    is RendererNode.Column -> JsWidget.Column(
        modifiers = modifiers.toJsModifiers(),
        children = children.toJsChildren(),
        horizontalAlignment = props.horizontalAlignment?.toJs(),
        verticalArrangement = props.verticalArrangement?.toJs(),
    )
    is RendererNode.Row -> JsWidget.Row(
        modifiers = modifiers.toJsModifiers(),
        children = children.toJsChildren(),
        verticalAlignment = props.verticalAlignment?.toJs(),
        horizontalArrangement = props.horizontalArrangement?.toJs(),
    )
    is RendererNode.Spacer -> JsWidget.Spacer(modifiers = modifiers.toJsModifiers())
    is RendererNode.Text -> JsWidget.Text(
        text = children.filterIsInstance<RendererNode.String>().joinToString("") { it.text },
        style = props.style?.toJs() ?: JsTypographyStyle.BODY_LARGE_REGULAR,
        color = props.color?.toJs(),
        modifiers = modifiers.toJsModifiers(),
    )
    is RendererNode.Button -> JsWidget.Button(
        text = props.text,
        onClick = props.clickAction,
        variant = props.variant?.toJs() ?: JsButtonVariant.PRIMARY,
        enabled = props.enabled ?: true,
        loading = props.loading ?: false,
        modifiers = modifiers.toJsModifiers(),
    )
    is RendererNode.TextField -> JsWidget.TextField(
        value = props.text,
        onValueChange = props.valueChangeAction,
        placeholder = props.placeholder,
        label = props.label,
        enabled = props.enabled ?: true,
        modifiers = modifiers.toJsModifiers(),
    )
    is RendererNode.Image -> JsWidget.Image(
        source = props.source.toJs(),
        fit = props.fit?.toJs() ?: JsImageFit.FILL,
        modifiers = modifiers.toJsModifiers(),
    )
    is RendererNode.Effect -> JsWidget.Effect(
        effect = props.effect.toJs(),
        children = children.toJsChildren(),
    )
}

private fun List<RendererNode>.toJsChildren(): List<JsWidget> =
    filterNot { it is RendererNode.Nil || it is RendererNode.String }.map { it.toJsWidget() }

private fun List<Modifier>.toJsModifiers(): List<JsModifier> = buildList {
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

private fun Shape.toJs(): JsShape = when (this) {
    is Shape.Rounded -> JsShape.Rounded(radius = v1.toDp())
    is Shape.Circle -> JsShape.Circle
    is Shape.Square -> JsShape.Square
}

private fun ImageSource.toJs(): JsImageSource = when (this) {
    is ImageSource.Bulletin -> JsImageSource.Bulletin(v1)
    is ImageSource.Archive -> JsImageSource.Archive(v1)
}

private fun ImageFit.toJs(): JsImageFit = when (this) {
    ImageFit.NONE -> JsImageFit.NONE
    ImageFit.FILL -> JsImageFit.FILL
    ImageFit.COVER -> JsImageFit.COVER
    ImageFit.CONTAIN -> JsImageFit.CONTAIN
    ImageFit.SCALE_DOWN -> JsImageFit.SCALE_DOWN
}

private fun Effect.toJs(): JsEffect = when (this) {
    Effect.RAINBOW -> JsEffect.RAINBOW
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
