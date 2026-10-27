package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcelable
import kotlinx.parcelize.Parcelize
import uniffi.truapi.ProductId

@Parcelize
class ProductSettingsPayload(
    val productId: ProductId
) : Parcelable
