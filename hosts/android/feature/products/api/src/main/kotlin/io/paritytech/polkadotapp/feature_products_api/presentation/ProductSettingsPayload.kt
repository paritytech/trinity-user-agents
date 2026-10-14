package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

@Parcelize
class ProductSettingsPayload(
    val productId: String
) : Parcelable
