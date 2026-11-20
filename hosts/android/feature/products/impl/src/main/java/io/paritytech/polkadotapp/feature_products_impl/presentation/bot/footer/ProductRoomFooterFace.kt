package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

/**
 * What a docked card shows, as far as the hand-back after a slide goes.
 *
 * A product that docks a slide to confirm in the footer keeps the same card while the slide runs:
 * its tree moves from the idle slider to the slid one and on to the one waiting on the product, and
 * the slider has to keep its place through all of them. When the product is done, the same card's
 * tree turns into an ordinary card, and that change of face is what the footer crossfades.
 */
internal enum class ProductRoomFooterFace {
    /** A tree holding a slide to confirm, in any of its states. */
    SLIDE,

    /** Any other tree, such as the card a slide hands the footer back to. */
    CARD,
    ;

    companion object {
        fun of(widget: JsWidget): ProductRoomFooterFace = if (widget.holdsSlide()) SLIDE else CARD

        private fun JsWidget.holdsSlide(): Boolean = when (this) {
            is JsWidget.SlideToConfirm -> true
            // A product-drawn slider keeps its place through its trees the same way.
            is JsWidget.Gesture -> true
            is JsWidget.Box -> children.any { it.holdsSlide() }
            is JsWidget.Column -> children.any { it.holdsSlide() }
            is JsWidget.Row -> children.any { it.holdsSlide() }
            else -> false
        }
    }
}
