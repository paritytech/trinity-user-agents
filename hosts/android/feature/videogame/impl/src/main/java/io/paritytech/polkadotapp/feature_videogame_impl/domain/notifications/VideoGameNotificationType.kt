package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.os.Parcelable
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductIdParceler
import kotlinx.parcelize.Parcelize
import kotlinx.parcelize.TypeParceler
import uniffi.truapi.ProductId

@Parcelize
sealed interface VideoGameNotificationType : Parcelable {
    @TypeParceler<ProductId, ProductIdParceler>
    data class ProductGameStartsSoon(val productId: ProductId) : VideoGameNotificationType
}
