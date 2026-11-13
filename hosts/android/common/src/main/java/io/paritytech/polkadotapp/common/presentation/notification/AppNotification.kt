package io.paritytech.polkadotapp.common.presentation.notification

import androidx.compose.runtime.Immutable

@Immutable
sealed interface AppNotification {
    val message: String

    data class Success(override val message: String) : AppNotification
    data class Error(override val message: String) : AppNotification

    /** A notice the user can tap to act on; it stays up longer than a confirmation. */
    class Tappable(override val message: String, val onTap: () -> Unit) : AppNotification
}
