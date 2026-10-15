package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import kotlinx.coroutines.flow.StateFlow

interface PocketFacePreviewContract {
    val state: StateFlow<PocketFacePreviewState>

    /** Finds the drawn face's images, so the preview shows them the way a published card would. */
    val imageResolver: JsImageResolver

    fun onBackClick()

    fun onUrlChanged(url: String)

    fun onDrawClick()
}
