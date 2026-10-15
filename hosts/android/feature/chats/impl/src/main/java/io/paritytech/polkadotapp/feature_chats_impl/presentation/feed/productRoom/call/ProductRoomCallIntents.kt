package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Bundle
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import javax.inject.Inject

/** The intents of the notification and of the in-app banner of a ringing product room. */
class ProductRoomCallIntents @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val deeplinkMapper: ChatDeeplinkMapper,
) {
    /** The tag of the notification of the room. */
    fun tag(chatId: ChatId): String = "product-room-call.${chatId.hex()}"

    /** Opens the room through the chat deeplink of the app. */
    fun openRoomIntent(chatId: ChatId): Intent = Intent(Intent.ACTION_VIEW, deeplinkMapper.toDeeplink(chatId))
        .setPackage(context.packageName)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    fun openRoom(chatId: ChatId): PendingIntent =
        PendingIntent.getActivity(context, chatId.hashCode(), openRoomIntent(chatId), FLAGS)

    // An activity, not a receiver: an app cannot start an activity from a receiver of a notification action.
    fun answer(chatId: ChatId, callId: String): PendingIntent = PendingIntent.getActivity(
        context,
        chatId.hashCode() + 1,
        Intent(context, ProductCallAnswerActivity::class.java).withCall(chatId, callId),
        FLAGS,
    )

    fun decline(chatId: ChatId, callId: String): PendingIntent = PendingIntent.getBroadcast(
        context,
        chatId.hashCode() + 2,
        Intent(context, ProductCallDeclineReceiver::class.java).withCall(chatId, callId),
        FLAGS,
    )

    private fun Intent.withCall(chatId: ChatId, callId: String) =
        putExtra(EXTRA_CHAT_ID, chatId.value.value).putExtra(EXTRA_CALL_ID, callId)

    internal companion object {
        const val EXTRA_CHAT_ID = "product_room_call.chat_id"
        const val EXTRA_CALL_ID = "product_room_call.call_id"
        const val FLAGS = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT

        fun Intent.call(): Pair<ChatId, String>? {
            val chatId = getByteArrayExtra(EXTRA_CHAT_ID)?.let(ChatId::fromRawValue) ?: return null
            val callId = getStringExtra(EXTRA_CALL_ID) ?: return null
            return chatId to callId
        }
    }
}

@EntryPoint
@InstallIn(SingletonComponent::class)
internal interface ProductRoomCallEntryPoint {
    fun productRoomCallAlerts(): ProductRoomCallAlerts

    fun productRoomCallIntents(): ProductRoomCallIntents
}

private fun Context.callEntryPoint() =
    EntryPointAccessors.fromApplication(applicationContext, ProductRoomCallEntryPoint::class.java)

/** Answer in the notification: the product gets `CallAnswered`, and the room opens. The activity shows nothing. */
class ProductCallAnswerActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        with(ProductRoomCallIntents) { intent.call() }?.let { (chatId, callId) ->
            val entryPoint = callEntryPoint()
            entryPoint.productRoomCallAlerts().answerFromNotification(chatId, callId, accept = true)
            startActivity(entryPoint.productRoomCallIntents().openRoomIntent(chatId))
        }
        finish()
    }
}

/** Decline in the notification: the product gets `CallAnswered` with `accept: false`. */
class ProductCallDeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        with(ProductRoomCallIntents) { intent.call() }?.let { (chatId, callId) ->
            context.callEntryPoint().productRoomCallAlerts().answerFromNotification(chatId, callId, accept = false)
        }
    }
}
