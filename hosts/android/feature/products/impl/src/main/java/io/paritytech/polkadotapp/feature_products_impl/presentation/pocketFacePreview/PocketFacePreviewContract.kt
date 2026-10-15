package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import kotlinx.coroutines.flow.StateFlow

interface PocketFacePreviewContract {
    val state: StateFlow<PocketFacePreviewState>

    val imageResolver: JsImageResolver

    fun onBackClick()

    fun onUrlChanged(url: String)

    fun onDrawClick()
}
