package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcelable
import kotlinx.parcelize.Parcelize
import kotlinx.parcelize.TypeParceler
import uniffi.truapi.ProductId

@Parcelize
@TypeParceler<ProductId, ProductIdParceler>
class ProductSettingsPayload(
    val productId: ProductId
) : Parcelable
