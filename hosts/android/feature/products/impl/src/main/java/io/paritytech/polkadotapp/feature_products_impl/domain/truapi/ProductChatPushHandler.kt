package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationHandler
import timber.log.Timber
import uniffi.truapi.ChatPushContent
import uniffi.truapi.ChatPushMediaKind
import uniffi.truapi.ChatPushPreview
import uniffi.truapi.ChatWakeReason
import java.math.BigInteger
import javax.inject.Inject

/** Opens the payload of a chat push with the key of its route (the core's A.15 decoder). */
fun interface ChatPushDecoder {
    fun decode(key: ByteArray, message: ByteArray): ChatPushPreview?
}

class CoreChatPushDecoder @Inject constructor() : ChatPushDecoder {
    override fun decode(key: ByteArray, message: ByteArray): ChatPushPreview? = uniffi.truapi.decodeChatPush(key, message)
}

/**
 * Shows the pushes of the chat product's contacts, which keep the native chat's shape (`pushId`, `message`), as the
 * native chat shows them: a message worded as its kind, an offer as the incoming call at once. A blocked contact shows
 * nothing, as in the native chat. It then wakes the chat worker, which takes the message from the statement store and
 * takes the answer to a call.
 */
class ProductChatPushHandler @Inject constructor(
    private val routes: ProductChatPushRoutes,
    private val decoder: ChatPushDecoder,
    private val notifications: PersonChatNotifications,
    private val contacts: PersonChatContacts,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
    // Lazy: the runtime boots on first use, and a push must not boot it just to be checked.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : PushNotificationHandler {
    override val priority: Int = 1

    override suspend fun canHandle(data: Map<String, String>): Boolean = data[PUSH_ID]?.let(routes::find) != null

    override suspend fun handle(data: Map<String, String>) {
        val route = data[PUSH_ID]?.let(routes::find) ?: return
        val message = data[MESSAGE]?.let { runCatching { it.fromHex() }.getOrNull() } ?: return
        // A push with nothing to show, such as the hang-up of a ringing call, still wakes the product, as a native push
        // still reaches its chat session.
        val preview = decoder.decode(route.key, message) ?: return wake(route)

        val chatId = ProductChatIdParameter(route.roomId).toChatId(ProductId.fromStoredValue(route.productId).toChatExtensionId())
        val contact = contacts.getContacts().firstOrNull { it.chatId == chatId }
        if (contact?.blocked == true) return
        val senderName = contact?.let { it.username ?: fallbackUsernameGenerator.generateFromAccountId(it.accountId) } ?: route.title

        when (preview) {
            is ChatPushPreview.Message -> preview.content.toPersonChatContent()?.let { notifications.showMessage(chatId, senderName, it) }
            is ChatPushPreview.Call -> notifications.ringCall(chatId, senderName, preview.messageId, preview.video)
        }
        wake(route)
    }

    private suspend fun wake(route: ProductChatPushRoute) {
        val runtime = runtimeProvider.get().runtime().getOrElse {
            Timber.w(it, "TrUAPI chat push: no runtime to wake %s", route.productId)
            return
        }
        runCatching { runtime.wakeProduct(route.productId, ChatWakeReason.Notification(route.roomId)) }
            .onFailure { Timber.w(it, "TrUAPI chat push: %s was not woken", route.productId) }
    }

    private companion object {
        const val PUSH_ID = "pushId"
        const val MESSAGE = "message"
    }
}

/** The kinds that the native chat shows, or null for one it shows nothing for (an edit, a removed reaction, a leave). */
internal fun ChatPushContent.toPersonChatContent(): PersonChatPushContent? = when (this) {
    is ChatPushContent.Text -> PersonChatPushContent.Text(text)
    is ChatPushContent.RichText -> richText(text, media)
    is ChatPushContent.Reply -> richText(text, media)
    is ChatPushContent.Reaction -> PersonChatPushContent.Reaction(emoji)
    ChatPushContent.ContactAdded -> PersonChatPushContent.ContactAdded
    ChatPushContent.RequestAccepted -> PersonChatPushContent.RequestAccepted
    // The native Android chat has no legacy Send message, so it shows one as unsupported.
    is ChatPushContent.Payment -> if (legacy) PersonChatPushContent.Unsupported else PersonChatPushContent.Payment(Balance(BigInteger(amount)))
    ChatPushContent.Unsupported -> PersonChatPushContent.Unsupported
    is ChatPushContent.Edited,
    ChatPushContent.ReactionRemoved,
    ChatPushContent.LeftChat,
    -> null
}

// The native chat shows nothing for a message without text or attachment.
private fun richText(text: String?, media: List<ChatPushMediaKind>): PersonChatPushContent? {
    val attachment = media.firstOrNull()?.let {
        when (it) {
            ChatPushMediaKind.IMAGE -> MessageAttachmentType.IMAGE
            ChatPushMediaKind.VIDEO -> MessageAttachmentType.VIDEO
            ChatPushMediaKind.FILE -> MessageAttachmentType.FILE
        }
    }
    if (text == null && attachment == null) return null
    return PersonChatPushContent.RichText(text, attachment)
}
