package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsUiEventHandler
import kotlinx.coroutines.flow.StateFlow

/**
 * Everything one product card needs to draw and drive its face: the stream it wears, where a press
 * inside it goes, and how its images are found. Built once per card and shared by every copy of it
 * on screen, so an expanding card does not open a second stream or fetch the same image again, and
 * draws from its first frame the face the card already wears.
 */
class ProductFaceBindings(
    val face: StateFlow<JsWidget?>,
    val onFaceAction: JsUiEventHandler,
    val imageResolver: JsImageResolver,
)
