package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsBlendingMode
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsShape
import org.junit.Assert.assertEquals
import org.junit.Test

class JsModifierOrderTest {
    private val opacity = JsModifier.Opacity(alpha = 128)
    private val blending = JsModifier.BlendingMode(JsBlendingMode.MULTIPLY)
    private val background = JsModifier.Background(JsColor.BG_SURFACE_CONTAINER, JsShape.Square)
    private val border = JsModifier.Border(width = 1, color = JsColor.FG_TERTIARY, shape = JsShape.Square)

    // Compose applies modifiers outside-in, so one applied after the background covers only what
    // comes later. A half-transparent card whose background stayed opaque is the visible symptom.
    @Test
    fun `opacity and blending come before the surface they are meant to cover`() {
        val ordered = listOf(background, opacity, border, blending).orderedForCompose()

        assertEquals(listOf(opacity, blending, background, border), ordered)
    }

    @Test
    fun `the frame still reads outside-in, margin first and size last`() {
        val margin = JsModifier.Margin(all = 8)
        val padding = JsModifier.Padding(all = 4)
        val size = JsModifier.Size(width = 40, height = null, minWidth = null, minHeight = null)

        val ordered = listOf(size, padding, background, margin).orderedForCompose()

        assertEquals(listOf(margin, background, padding, size), ordered)
    }
}
