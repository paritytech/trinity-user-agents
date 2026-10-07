package io.paritytech.polkadotapp.common.presentation.notifications

import androidx.annotation.StringRes
import androidx.core.app.NotificationManagerCompat
import io.paritytech.polkadotapp.common.R as RCommon

enum class PolkadotNotificationChannel(
    val id: String,
    @param:StringRes val nameRes: Int,
    @param:StringRes val descriptionRes: Int,
    val importance: Int,
    val vibrationPattern: LongArray? = null,
    val isSilent: Boolean = false,
) {
    CHAT(
        id = "notification_channel_id_chat",
        nameRes = RCommon.string.notification_channel_name_chat,
        descriptionRes = RCommon.string.notification_channel_description_chat,
        importance = NotificationManagerCompat.IMPORTANCE_HIGH
    ),

    CALLS(
        // Incoming-call ringtone is owned by CallAlertManager, so the channel itself stays silent.
        // Channel settings are immutable post-creation, hence the id bump from the original "calls".
        id = "calls_ringtone",
        nameRes = RCommon.string.notification_channel_name_calls,
        descriptionRes = RCommon.string.notification_channel_description_calls,
        importance = NotificationManagerCompat.IMPORTANCE_MAX,
        isSilent = true,
    ),

    VIDEO_GAME(
        id = "video_game_notifications",
        nameRes = RCommon.string.notification_channel_name_video_game,
        descriptionRes = RCommon.string.notification_channel_description_video_game,
        importance = NotificationManagerCompat.IMPORTANCE_DEFAULT
    ),

    VIDEO_GAME_ALARM(
        id = "video_game_alarms",
        nameRes = RCommon.string.notification_channel_name_video_game_alarm,
        descriptionRes = RCommon.string.notification_channel_description_video_game_alarm,
        importance = NotificationManagerCompat.IMPORTANCE_MAX,
        vibrationPattern = longArrayOf(0, 200, 150, 200)
    ),

    PRODUCTS(
        id = "product_notifications",
        nameRes = RCommon.string.notification_channel_name_products,
        descriptionRes = RCommon.string.notification_channel_description_products,
        importance = NotificationManagerCompat.IMPORTANCE_HIGH
    )
}
