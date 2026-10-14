package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import android.app.Notification
import android.content.Context
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.deeplink.ChatListDeeplinkMapper
import javax.inject.Inject

private const val GROUP_TAG = "GROUP_CHAT_TAG"

internal class ChatNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val chatDeeplinkMapper: ChatDeeplinkMapper,
    private val chatListDeeplinkMapper: ChatListDeeplinkMapper,
) : NotificationPublisher(context, intentProvider) {
    fun publishNewMessageReceived(
        chatId: ChatId,
        username: String,
        text: String
    ) {
        val channel = PolkadotNotificationChannel.CHAT
        val notificationId = chatId.toNotificationId()

        publishNewMessageNotification(
            notificationId = notificationId,
            chatId = chatId,
            text = text,
            username = username,
            channelId = channel.id,
            channel = channel
        )
        publishSummaryNotification(channel.id, channel)
    }

    fun cancelChatNotification(chatId: ChatId) {
        val notificationId = chatId.toNotificationId()

        cancel(notificationId)

        if (hasOtherChatNotifications(notificationId).not()) {
            cancel(GROUP_TAG.hashCode())
        }
    }

    private fun publishNewMessageNotification(
        notificationId: Int,
        chatId: ChatId,
        text: String,
        username: String,
        channelId: String,
        channel: PolkadotNotificationChannel
    ) {
        val me = Person.Builder().setName("Me").build() // doesn't matter since we cannot respond from push

        val newMessage = NotificationCompat.MessagingStyle.Message(
            text,
            System.currentTimeMillis(),
            Person.Builder().setName(username).build()
        )

        val currentNotification = getNotification(notificationId)
        val messages = (currentNotification?.extractMessages() ?: emptyList()) + newMessage

        val style = NotificationCompat.MessagingStyle(me)

        for (message in messages) {
            style.addMessage(message)
        }

        val deeplink = chatDeeplinkMapper.toDeeplink(chatId = chatId)

        val builder = NotificationCompat.Builder(appContext, channelId)
            .setupDefaultNotification(deepLink = deeplink)
            .setStyle(style)
            .setAutoCancel(true)
            .setSortKey((Long.MAX_VALUE - newMessage.timestamp).toString())
            .setGroup(GROUP_TAG)
            .setCategory(Notification.CATEGORY_MESSAGE)
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_SUMMARY)

        publish(notificationId, channel, builder.build())
    }

    private fun publishSummaryNotification(
        channelId: String,
        channel: PolkadotNotificationChannel
    ) {
        val deeplink = chatListDeeplinkMapper.toDeeplink()
        val totalUnreadCount = countTotalUnreadMessages()

        val builder = NotificationCompat.Builder(appContext, channelId)
            .setupDefaultNotification(deepLink = deeplink)
            .setStyle(NotificationCompat.InboxStyle())
            .setGroup(GROUP_TAG)
            .setGroupSummary(true)
            .setAutoCancel(true)
            .setNumber(totalUnreadCount)

        publish(GROUP_TAG.hashCode(), channel, builder.build())
    }

    private fun countTotalUnreadMessages(): Int {
        return activeNotifications
            .filter { it.notification.group == GROUP_TAG }
            .filter { (it.notification.flags and Notification.FLAG_GROUP_SUMMARY) == 0 }
            .sumOf { notification -> notification.notification.extractMessages().size }
    }

    private fun Notification.extractMessages(): List<NotificationCompat.MessagingStyle.Message> {
        val messagingStyle = NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(this)
        return messagingStyle?.messages ?: emptyList()
    }

    private fun hasOtherChatNotifications(notificationToRemoveId: Int): Boolean =
        activeNotifications.any { statusBarNotification ->
            val notification = statusBarNotification.notification

            val isSameGroup = notification.group == GROUP_TAG
            val isNotSummary = (notification.flags and Notification.FLAG_GROUP_SUMMARY) == 0

            val isNotTheDeletedOne = statusBarNotification.id != notificationToRemoveId

            isSameGroup && isNotSummary && isNotTheDeletedOne
        }

    private fun ChatId.toNotificationId(): Int = this.value.value.contentHashCode()
}
