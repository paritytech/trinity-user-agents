package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketFacePreview

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

@Immutable
data class PocketFacePreviewState(
    val url: String,
    /** Null until a face has been asked for, so the screen opens on its instructions. */
    val face: LoadingState<JsWidget>?,
)
