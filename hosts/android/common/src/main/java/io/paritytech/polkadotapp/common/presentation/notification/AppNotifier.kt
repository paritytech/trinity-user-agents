package io.paritytech.polkadotapp.common.presentation.notification

import androidx.compose.runtime.Stable
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import javax.inject.Inject
import javax.inject.Singleton

@Stable
interface AppNotifier {
    val notifications: Flow<AppNotification>

    fun notify(notification: AppNotification)
}

fun AppNotifier.success(message: String) = notify(AppNotification.Success(message))
fun AppNotifier.error(message: String) = notify(AppNotification.Error(message))

@Singleton
class RealAppNotifier @Inject constructor() : AppNotifier {
    private val _notifications = MutableSharedFlow<AppNotification>(
        extraBufferCapacity = 4,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    override val notifications: Flow<AppNotification> = _notifications

    override fun notify(notification: AppNotification) {
        _notifications.tryEmit(notification)
    }
}
