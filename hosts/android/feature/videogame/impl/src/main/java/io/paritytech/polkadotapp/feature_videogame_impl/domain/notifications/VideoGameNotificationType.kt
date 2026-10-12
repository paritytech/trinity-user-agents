package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.parcelize.Parcelize

@Parcelize
sealed interface VideoGameNotificationType : Parcelable {
    data class RegistrationOpened(val timestamp: Timestamp) : VideoGameNotificationType
    data object WaitingRoomAvailable : VideoGameNotificationType
    data object GameAboutToStart : VideoGameNotificationType
    data object GameStartsSoon : VideoGameNotificationType
    data class ProductGameStartsSoon(val productId: String) : VideoGameNotificationType
}
