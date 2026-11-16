package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductRoomFooterFaceTest {
    private val slider = JsWidget.SlideToConfirm(
        text = "Slide to register",
        secondaryText = "Today at 20:00",
        confirmedText = "Registered",
        onConfirm = "register",
        modifiers = listOf(JsModifier.FillMaxWidth(), JsModifier.Size(height = 80)),
    )
    private val registeredCard = JsWidget.Column(
        children = listOf(
            JsWidget.Text("Registered"),
            JsWidget.Text("Today at 20:00"),
        ),
    )

    @Test
    fun `a docked slider is the slide face`() {
        assertEquals(ProductRoomFooterFace.SLIDE, ProductRoomFooterFace.of(slider))
    }

    // Jollity wraps its controls in boxes, columns and rows; a slider anywhere in the tree is still the slide.
    @Test
    fun `a slider inside a box, a column or a row is the slide face`() {
        assertEquals(ProductRoomFooterFace.SLIDE, ProductRoomFooterFace.of(JsWidget.Box(children = listOf(slider))))
        assertEquals(ProductRoomFooterFace.SLIDE, ProductRoomFooterFace.of(JsWidget.Column(children = listOf(JsWidget.Spacer(), slider))))
        assertEquals(
            ProductRoomFooterFace.SLIDE,
            ProductRoomFooterFace.of(JsWidget.Box(children = listOf(JsWidget.Row(children = listOf(slider))))),
        )
    }

    // The slid control waiting on the product must keep its face, or the footer would crossfade the thumb away mid-wait.
    @Test
    fun `a slid slider and one waiting on the product keep the slide face`() {
        assertEquals(ProductRoomFooterFace.SLIDE, ProductRoomFooterFace.of(slider.copy(loading = true, onConfirm = null)))
        assertEquals(ProductRoomFooterFace.SLIDE, ProductRoomFooterFace.of(slider.copy(confirmed = true, onConfirm = null)))
    }

    @Test
    fun `the card a slide hands back to is the card face`() {
        assertEquals(ProductRoomFooterFace.CARD, ProductRoomFooterFace.of(registeredCard))
        assertEquals(ProductRoomFooterFace.CARD, ProductRoomFooterFace.of(JsWidget.Button(text = "Register", onClick = "register")))
    }
}
