package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.os.Parcelable
import kotlinx.parcelize.Parcelize
import uniffi.truapi.ProductId

@Parcelize
sealed interface VideoGameNotificationType : Parcelable {
    data class ProductGameStartsSoon(val productId: ProductId) : VideoGameNotificationType
}
