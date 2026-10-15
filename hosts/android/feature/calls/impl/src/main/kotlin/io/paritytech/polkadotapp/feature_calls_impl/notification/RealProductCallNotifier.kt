package io.paritytech.polkadotapp.feature_calls_impl.notification

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallNotifier
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductRingingNotification
import timber.log.Timber
import javax.inject.Inject

class RealProductCallNotifier @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : ProductCallNotifier {
    private val notifications = CallNotificationManager(context)

    // The notifier does not ask for the permission: a ringing call is no time to show a system prompt.
    @SuppressLint("MissingPermission")
    override fun showRinging(notification: ProductRingingNotification): Boolean {
        if (!canNotify()) {
            Timber.i("The ringing call of %s has no notification: the user does not allow notifications", notification.tag)
            return false
        }
        NotificationManagerCompat.from(context)
            .notify(notification.tag, NOTIFICATION_ID, notifications.createProductRingingNotification(notification))
        return true
    }

    override fun cancelRinging(tag: String) {
        NotificationManagerCompat.from(context).cancel(tag, NOTIFICATION_ID)
    }

    private fun canNotify(): Boolean {
        val permitted = Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        return permitted && NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

    private companion object {
        /** Each room has its own tag, so one id serves every room. */
        const val NOTIFICATION_ID = 0x50524f44
    }
}
