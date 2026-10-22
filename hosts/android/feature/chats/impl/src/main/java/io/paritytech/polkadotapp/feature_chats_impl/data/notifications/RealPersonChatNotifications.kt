package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomRinger
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatMessageMappingHelper
import io.paritytech.polkadotapp.feature_chats_impl.utils.emojiRes
import io.paritytech.polkadotapp.feature_chats_impl.utils.nameRes
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

// The wording of each kind is the one of ChatPushNotificationHandler, which shows the pushes of the native chat.
internal class RealPersonChatNotifications @Inject constructor(
    private val chatNotificationPublisher: ChatNotificationPublisher,
    private val chatActiveTracker: ChatActiveTracker,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val tokenAmountFormatter: TokenAmountFormatter,
    private val messageMappingHelper: ChatMessageMappingHelper,
    private val ringer: ProductRoomRinger,
    @param:ApplicationContext private val appContext: Context,
) : PersonChatNotifications {
    override suspend fun showMessage(chatId: ChatId, senderName: String, content: PersonChatPushContent) {
        val onScreen = chatActiveTracker.getActive() == chatId && appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND
        if (onScreen) return

        val text = content.toMessageText() ?: return
        chatNotificationPublisher.publishNewMessageReceived(chatId = chatId, username = senderName, text = text)
    }

    override suspend fun ringCall(chatId: ChatId, callerName: String, callId: String, video: Boolean) {
        ringer.ringPushedCall(chatId, callId, callerName, video)
    }

    private suspend fun PersonChatPushContent.toMessageText(): String? = when (this) {
        is PersonChatPushContent.Text -> text

        is PersonChatPushContent.RichText -> when (val attachment = attachment) {
            null -> text
            else -> {
                val emoji = appContext.getString(attachment.emojiRes())
                if (text != null) "$emoji $text" else "$emoji ${appContext.getString(attachment.nameRes())}"
            }
        }

        is PersonChatPushContent.Reaction -> appContext.getString(RCommon.string.chat_notification_reaction_added, emoji)

        PersonChatPushContent.ContactAdded -> appContext.getString(RCommon.string.chat_notification_contact_added)

        PersonChatPushContent.RequestAccepted -> appContext.getString(RCommon.string.chat_notification_request_approved)

        is PersonChatPushContent.Payment -> paymentText(totalValue)

        PersonChatPushContent.Unsupported -> appContext.getString(RCommon.string.chat_message_unsupported)
    }

    private suspend fun paymentText(totalValue: Balance): String {
        val value = tokenAmountFormatter.formatTokenAmount(
            tokenAmount = messageMappingHelper.extractTokenAmount(totalValue),
            precision = RoundPrecision.DEFAULT
        )

        return appContext.getString(RCommon.string.chat_notification_payment, value)
    }
}
