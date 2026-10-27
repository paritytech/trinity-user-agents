package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcelable
import kotlinx.parcelize.Parcelize
import uniffi.truapi.ProductId

/** Navigation arg for the Pocket approval sheet: which published card the user is offered. */
@Parcelize
data class PocketAddCardPayload(
    val productId: ProductId,
    val cardId: String,
) : Parcelable
