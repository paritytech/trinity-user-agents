package io.paritytech.polkadotapp.feature_chats_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatMessageNotificationSentRepository
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import timber.log.Timber
import javax.inject.Inject

interface ChatPushNotificationsSender {
    suspend fun sendPushNotificationOnce(
        messageId: ChatMessageId,
        platformToken: String,
        operatingSystem: OperatingSystem,
        pushId: ByteArray,
        encryptedMessage: ByteArray,
        isVoIP: Boolean = false
    )
}

class RealChatPushNotificationsSender @Inject constructor(
    private val pushNotificationsHelper: PushNotificationsHelper,
    private val chatMessageNotificationSentRepository: ChatMessageNotificationSentRepository
) : ChatPushNotificationsSender {
    override suspend fun sendPushNotificationOnce(
        messageId: ChatMessageId,
        platformToken: String,
        operatingSystem: OperatingSystem,
        pushId: ByteArray,
        encryptedMessage: ByteArray,
        isVoIP: Boolean
    ) {
        val pushWasSent = chatMessageNotificationSentRepository.wasNotificationSent(messageId)
        Timber.d("ChatPushNotificationsSender: messageId=$messageId, pushWasSent=$pushWasSent")
        if (pushWasSent) return

        pushNotificationsHelper.sendNotify(
            platformToken = platformToken,
            operatingSystem = operatingSystem,
            pushId = pushId,
            encryptedMessage = encryptedMessage,
            isVoIP = isVoIP
        )
            .onSuccess {
                Timber.d("ChatPushNotificationsSender: messageId=$messageId, pushNotificationsHelper.sendNotify onSuccess")
                chatMessageNotificationSentRepository.saveNotificationSent(messageId)
            }
            .onFailure {
                Timber.e(it, "ChatPushNotificationsSender: messageId=$messageId, pushNotificationsHelper.sendNotify onFailure")
            }
    }
}
