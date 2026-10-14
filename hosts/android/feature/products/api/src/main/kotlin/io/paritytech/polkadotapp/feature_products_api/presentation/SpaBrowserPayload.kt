package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

/** Navigation arg for the SPA browser: which product session to activate on open. */
sealed interface SpaBrowserPayload : Parcelable {
    @Parcelize
    data class ByProductId(val productId: String) : SpaBrowserPayload

    @Parcelize
    data class ByUrl(val url: String) : SpaBrowserPayload
}
