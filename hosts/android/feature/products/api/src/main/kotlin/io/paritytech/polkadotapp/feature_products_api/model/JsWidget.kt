package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

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

    /** Applies a host-drawn effect to its children. */
    @Serializable
    @SerialName("effect")
    data class Effect(
        val effect: JsEffect,
        val children: List<JsWidget> = emptyList(),
    ) : JsWidget
}
