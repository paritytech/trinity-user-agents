package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.Transient

@Serializable
sealed interface JsWidget {
    @Serializable
    @SerialName("box")
    data class Box(
        val modifiers: List<JsModifier> = emptyList(),
        val children: List<JsWidget> = emptyList(),
        val contentAlignment: JsAlignment? = null,
    ) : JsWidget

    @Serializable
    @SerialName("column")
    data class Column(
        val modifiers: List<JsModifier> = emptyList(),
        val children: List<JsWidget> = emptyList(),
        val horizontalAlignment: JsHorizontalAlignment? = null,
        val verticalArrangement: JsArrangement? = null,
    ) : JsWidget

    @Serializable
    @SerialName("row")
    data class Row(
        val modifiers: List<JsModifier> = emptyList(),
        val children: List<JsWidget> = emptyList(),
        val verticalAlignment: JsVerticalAlignment? = null,
        val horizontalArrangement: JsArrangement? = null,
    ) : JsWidget

    @Serializable
    @SerialName("spacer")
    data class Spacer(
        val modifiers: List<JsModifier> = emptyList(),
    ) : JsWidget

    @Serializable
    @SerialName("text")
    data class Text(
        val text: String,
        val style: JsTypographyStyle = JsTypographyStyle.BODY_LARGE_REGULAR,
        val color: JsColor? = null,
        val modifiers: List<JsModifier> = emptyList(),
    ) : JsWidget

    /**
     * Interactive button widget.
     * @param text Button label
     * @param onClick Action ID registered in JS that will be invoked on click
     * @param variant Visual style variant
     * @param enabled Whether the button is interactive
     * @param loading Shows loading indicator instead of text
     */
    @Serializable
    @SerialName("button")
    data class Button(
        val text: String,
        val onClick: String? = null,
        val variant: JsButtonVariant = JsButtonVariant.PRIMARY,
        val enabled: Boolean = true,
        val loading: Boolean = false,
        val modifiers: List<JsModifier> = emptyList(),
    ) : JsWidget

    /**
     * Draws its children and reports taps and pans on them; no styling of its own. A keyed location pan keeps the
     * finger's strokes in [trail] for masks that follow it.
     * @param tapAction Action ID invoked on a tap that did not become a pan
     * @param pan The pan the node accepts, if any
     * @param accessibilityLabel What a screen reader's action on the node is called
     * @param enabled Whether the node accepts gestures
     * @param hapticOnComplete Whether a completed drag gives success feedback
     */
    @Serializable
    @SerialName("gesture")
    data class Gesture(
        @Transient val position: JsGesturePosition? = null,
        @Transient val trail: JsPanTrail? = null,
        val tapAction: String? = null,
        val pan: JsPanGesture? = null,
        val accessibilityLabel: String? = null,
        val enabled: Boolean = true,
        val hapticOnComplete: Boolean = false,
        val modifiers: List<JsModifier> = emptyList(),
        val children: List<JsWidget> = emptyList(),
        /** The gesture's key, by which nodes that fade with its pan find it. */
        val key: String? = null,
        val feel: JsPanFeel? = null,
    ) : JsWidget

    /**
     * Text input field widget.
     * @param value Current text value
     * @param onValueChange Action ID invoked when text changes (receives new value in payload)
     * @param placeholder Hint text when empty
     * @param label Optional label above the field
     * @param enabled Whether the field is editable
     */
    @Serializable
    @SerialName("textField")
    data class TextField(
        val value: String = "",
        val onValueChange: String? = null,
        val placeholder: String? = null,
        val label: String? = null,
        val enabled: Boolean = true,
        val modifiers: List<JsModifier> = emptyList(),
    ) : JsWidget

    /** Image sized by its modifiers; one the host cannot fetch draws as empty space. */
    @Serializable
    @SerialName("image")
    data class Image(
        val source: JsImageSource,
        val fit: JsImageFit = JsImageFit.FILL,
        val modifiers: List<JsModifier> = emptyList(),
    ) : JsWidget

    /**
     * Shader layers over [children]. [node] is its place among the tree's `Materials` nodes, depth
     * first, as the core numbers them. A surface without a materials renderer draws the children plain.
     */
    @Serializable
    @SerialName("materials")
    data class Materials(
        val node: Int,
        val children: List<JsWidget>,
    ) : JsWidget
}

/** Every keyed mask in the tree, by key; of two with the same key, the first. */
fun JsWidget.keyedMasks(): Map<String, JsModifier.Mask> = buildMap { collectMasks(this) }

private fun JsWidget.collectMasks(into: MutableMap<String, JsModifier.Mask>) {
    val (modifiers, children) = when (this) {
        is JsWidget.Box -> modifiers to children
        is JsWidget.Column -> modifiers to children
        is JsWidget.Row -> modifiers to children
        is JsWidget.Gesture -> modifiers to children
        is JsWidget.Spacer -> modifiers to emptyList()
        is JsWidget.Text -> modifiers to emptyList()
        is JsWidget.Button -> modifiers to emptyList()
        is JsWidget.Materials -> emptyList<JsModifier>() to children
        is JsWidget.TextField -> modifiers to emptyList()
        is JsWidget.Image -> modifiers to emptyList()
    }
    modifiers.filterIsInstance<JsModifier.Mask>().lastOrNull()?.let { mask ->
        mask.key?.let { key -> if (key !in into) into[key] = mask }
    }
    children.forEach { it.collectMasks(into) }
}
