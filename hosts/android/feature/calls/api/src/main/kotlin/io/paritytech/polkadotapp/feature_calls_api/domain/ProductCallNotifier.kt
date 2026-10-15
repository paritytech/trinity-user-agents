package io.paritytech.polkadotapp.feature_calls_api.domain

import android.app.PendingIntent

/**
 * Shows the `CallStyle` notification of a product room that rings while the app is in the background (07-LATER
 * §11.2). A product call has no foreground service and no `ConnectionService` (D31): the notification opens the room.
 */
interface ProductCallNotifier {
    /** False when the user does not let the app show notifications. The notifier does not ask the user. */
    fun showRinging(notification: ProductRingingNotification): Boolean

    fun cancelRinging(tag: String)
}

/** The ringing call of one product room. [tag] identifies the room. */
data class ProductRingingNotification(
    val tag: String,
    val roomName: String,
    val video: Boolean,
    /** Opens the room. The notification also uses it as its full-screen intent. */
    val openIntent: PendingIntent,
    /** Answers the call and opens the room. */
    val answerIntent: PendingIntent,
    /** Declines the call. */
    val declineIntent: PendingIntent,
)
