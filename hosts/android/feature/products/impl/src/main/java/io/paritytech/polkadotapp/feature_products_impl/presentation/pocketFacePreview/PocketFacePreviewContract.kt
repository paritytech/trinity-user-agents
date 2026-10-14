package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import kotlinx.coroutines.flow.StateFlow

interface PocketFacePreviewContract {
    val state: StateFlow<PocketFacePreviewState>

    fun onBackClick()

    fun onUrlChanged(url: String)

    fun onDrawClick()
}
