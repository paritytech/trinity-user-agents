package io.paritytech.polkadotapp.feature_products_impl.domain.serialization.scale

import io.paritytech.polkadotapp.feature_products_api.model.JsAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsArrangement
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsVerticalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

/**
 * Maps decoded SCALE widget models to the existing JsWidget model used by Compose renderers.
 * This avoids restructuring JsWidget and touching all Compose renderers.
 */
fun ScaleWidget.toJsWidget(): JsWidget = when (this) {
    is ScaleWidget.Nil -> JsWidget.Spacer()
    is ScaleWidget.StringNode -> JsWidget.Text(text = value)
    is ScaleWidget.Box -> JsWidget.Box(
        modifiers = modifiers.toJsModifiers(),
        children = children.mapNotNull { it.toJsWidgetOrNull() },
        contentAlignment = props.contentAlignment?.toJsAlignment(),
    )
    is ScaleWidget.Column -> JsWidget.Column(
        modifiers = modifiers.toJsModifiers(),
        children = children.mapNotNull { it.toJsWidgetOrNull() },
        horizontalAlignment = props.horizontalAlignment?.toJsHorizontalAlignment(),
        verticalArrangement = props.verticalArrangement?.toJsArrangement(),
    )
    is ScaleWidget.Row -> JsWidget.Row(
        modifiers = modifiers.toJsModifiers(),
        children = children.mapNotNull { it.toJsWidgetOrNull() },
        verticalAlignment = props.verticalAlignment?.toJsVerticalAlignment(),
        horizontalArrangement = props.horizontalArrangement?.toJsArrangement(),
    )
    is ScaleWidget.Spacer -> JsWidget.Spacer(
        modifiers = modifiers.toJsModifiers(),
    )
    is ScaleWidget.Text -> JsWidget.Text(
        text = children.extractText(),
        style = props.style?.toJsTypographyStyle() ?: JsTypographyStyle.BODY_LARGE_REGULAR,
        color = props.color?.toJsColor(),
        modifiers = modifiers.toJsModifiers(),
    )
    is ScaleWidget.Button -> JsWidget.Button(
        text = props.text,
        onClick = props.clickAction,
        variant = props.variant?.toJsButtonVariant() ?: JsButtonVariant.PRIMARY,
        enabled = props.enabled ?: true,
        loading = props.loading ?: false,
        modifiers = modifiers.toJsModifiers(),
    )
    is ScaleWidget.TextField -> JsWidget.TextField(
        value = props.text,
        onValueChange = props.valueChangeAction,
        placeholder = props.placeholder,
        label = props.label,
        enabled = props.enabled ?: true,
        modifiers = modifiers.toJsModifiers(),
    )
}

/**
 * Convert a ScaleWidget to JsWidget, returning null for StringNode children
 * (they are consumed by their parent as text content, not rendered independently).
 */
private fun ScaleWidget.toJsWidgetOrNull(): JsWidget? = when (this) {
    is ScaleWidget.StringNode -> null
    is ScaleWidget.Nil -> null
    else -> toJsWidget()
}

/**
 * Extract text content from children by finding StringNode children.
 */
private fun List<ScaleWidget>.extractText(): String {
    return filterIsInstance<ScaleWidget.StringNode>()
        .joinToString("") { it.value }
}

// --- Modifiers mapping ---

private fun List<ScaleModifier>.toJsModifiers(): List<JsModifier> {
    return buildList {
        // Collect size-related modifiers to merge into a single JsModifier.Size
        var width: Int? = null
        var height: Int? = null
        var minWidth: Int? = null
        var minHeight: Int? = null

        for (modifier in this@toJsModifiers) {
            when (modifier) {
                is ScaleModifier.Margin -> add(JsModifier.Margin(
                    top = modifier.value.top,
                    end = modifier.value.end,
                    bottom = modifier.value.bottom,
                    start = modifier.value.start,
                ))
                is ScaleModifier.Padding -> add(JsModifier.Padding(
                    top = modifier.value.top,
                    end = modifier.value.end,
                    bottom = modifier.value.bottom,
                    start = modifier.value.start,
                ))
                is ScaleModifier.Background -> add(JsModifier.Background(
                    color = modifier.value.color.toJsColor(),
                    shape = modifier.value.shape?.toJsShape(),
                ))
                is ScaleModifier.Border -> add(JsModifier.Border(
                    width = modifier.value.width.toInt(),
                    color = modifier.value.color.toJsColor(),
                    shape = modifier.value.shape?.toJsShape(),
                ))
                is ScaleModifier.Width -> width = modifier.value.toInt()
                is ScaleModifier.Height -> height = modifier.value.toInt()
                is ScaleModifier.MinWidth -> minWidth = modifier.value.toInt()
                is ScaleModifier.MinHeight -> minHeight = modifier.value.toInt()
                is ScaleModifier.FillWidth -> if (modifier.value) add(JsModifier.FillMaxWidth())
                is ScaleModifier.FillHeight -> if (modifier.value) add(JsModifier.FillMaxHeight())
            }
        }

        if (width != null || height != null || minWidth != null || minHeight != null) {
            add(JsModifier.Size(
                width = width,
                height = height,
                minWidth = minWidth,
                minHeight = minHeight,
            ))
        }
    }
}

// --- Shape mapping ---

private fun ScaleShape.toJsShape(): JsShape = when (this) {
    is ScaleShape.Rounded -> JsShape.Rounded(radius = radius.toInt())
    is ScaleShape.Circle -> JsShape.Circle
}

// --- Enum mappings ---

private fun ScaleColorToken.toJsColor(): JsColor = when (this) {
    ScaleColorToken.FG_PRIMARY -> JsColor.FG_PRIMARY
    ScaleColorToken.FG_SECONDARY -> JsColor.FG_SECONDARY
    ScaleColorToken.FG_TERTIARY -> JsColor.FG_TERTIARY
    ScaleColorToken.BG_SURFACE_MAIN -> JsColor.BG_SURFACE_MAIN
    ScaleColorToken.BG_SURFACE_CONTAINER -> JsColor.BG_SURFACE_CONTAINER
    ScaleColorToken.BG_SURFACE_NESTED -> JsColor.BG_SURFACE_NESTED
    ScaleColorToken.FG_SUCCESS -> JsColor.FG_SUCCESS
    ScaleColorToken.FG_ERROR -> JsColor.FG_ERROR
    ScaleColorToken.FG_WARNING -> JsColor.FG_WARNING
}

private fun ScaleTypographyStyle.toJsTypographyStyle(): JsTypographyStyle = when (this) {
    ScaleTypographyStyle.HEADLINE_LARGE -> JsTypographyStyle.HEADLINE_LARGE
    ScaleTypographyStyle.TITLE_MEDIUM_REGULAR -> JsTypographyStyle.TITLE_MEDIUM_REGULAR
    ScaleTypographyStyle.BODY_LARGE_REGULAR -> JsTypographyStyle.BODY_LARGE_REGULAR
    ScaleTypographyStyle.BODY_MEDIUM_REGULAR -> JsTypographyStyle.BODY_MEDIUM_REGULAR
    ScaleTypographyStyle.BODY_SMALL_REGULAR -> JsTypographyStyle.BODY_SMALL_REGULAR
}

private fun ScaleButtonVariant.toJsButtonVariant(): JsButtonVariant = when (this) {
    ScaleButtonVariant.PRIMARY -> JsButtonVariant.PRIMARY
    ScaleButtonVariant.SECONDARY -> JsButtonVariant.SECONDARY
    ScaleButtonVariant.TEXT -> JsButtonVariant.TEXT
}

private fun ScaleContentAlignment.toJsAlignment(): JsAlignment = when (this) {
    ScaleContentAlignment.TOP_START -> JsAlignment.TOP_START
    ScaleContentAlignment.TOP_CENTER -> JsAlignment.TOP_CENTER
    ScaleContentAlignment.TOP_END -> JsAlignment.TOP_END
    ScaleContentAlignment.CENTER_START -> JsAlignment.CENTER_START
    ScaleContentAlignment.CENTER -> JsAlignment.CENTER
    ScaleContentAlignment.CENTER_END -> JsAlignment.CENTER_END
    ScaleContentAlignment.BOTTOM_START -> JsAlignment.BOTTOM_START
    ScaleContentAlignment.BOTTOM_CENTER -> JsAlignment.BOTTOM_CENTER
    ScaleContentAlignment.BOTTOM_END -> JsAlignment.BOTTOM_END
}

private fun ScaleHorizontalAlignment.toJsHorizontalAlignment(): JsHorizontalAlignment = when (this) {
    ScaleHorizontalAlignment.START -> JsHorizontalAlignment.START
    ScaleHorizontalAlignment.CENTER -> JsHorizontalAlignment.CENTER
    ScaleHorizontalAlignment.END -> JsHorizontalAlignment.END
}

private fun ScaleVerticalAlignment.toJsVerticalAlignment(): JsVerticalAlignment = when (this) {
    ScaleVerticalAlignment.TOP -> JsVerticalAlignment.TOP
    ScaleVerticalAlignment.CENTER -> JsVerticalAlignment.CENTER
    ScaleVerticalAlignment.BOTTOM -> JsVerticalAlignment.BOTTOM
}

private fun ScaleArrangement.toJsArrangement(): JsArrangement = when (this) {
    ScaleArrangement.START -> JsArrangement.START
    ScaleArrangement.END -> JsArrangement.END
    ScaleArrangement.CENTER -> JsArrangement.CENTER
    ScaleArrangement.SPACE_BETWEEN -> JsArrangement.SPACE_BETWEEN
    ScaleArrangement.SPACE_AROUND -> JsArrangement.SPACE_AROUND
    ScaleArrangement.SPACE_EVENLY -> JsArrangement.SPACE_EVENLY
}
