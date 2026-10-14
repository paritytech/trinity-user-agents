package io.paritytech.polkadotapp.tools_push_notifications_impl

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationHandler
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.LocalPushTokenStorage
import kotlinx.coroutines.runBlocking
import timber.log.Timber
import javax.inject.Inject

@AndroidEntryPoint
class PushNotificationService : FirebaseMessagingService() {
    @Inject
    lateinit var handlers: Set<@JvmSuppressWildcards PushNotificationHandler>

    @Inject
    lateinit var pushTokenStorage: LocalPushTokenStorage

    override fun onNewToken(token: String) = runBlocking {
        Timber.d("FCM onNewToken: received new token")
        pushTokenStorage.saveValue(token)
    }

    override fun onMessageReceived(message: RemoteMessage) = runBlocking {
        for (handler in handlers) {
            val data = message.data

            if (handler.canHandle(data)) {
                handler.handle(data)
                break
            }
        }
    }
}
