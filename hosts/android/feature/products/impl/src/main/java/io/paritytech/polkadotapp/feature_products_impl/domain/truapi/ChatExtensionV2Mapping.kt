package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionButtonModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionsLayoutModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputPaymentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewTextModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatActionLayout
import uniffi.truapi.ChatActions
import uniffi.truapi.ChatActivity
import uniffi.truapi.ChatCommandSpec
import uniffi.truapi.ChatFile
import uniffi.truapi.ChatInputPayment
import uniffi.truapi.ChatMediaItem
import uniffi.truapi.ChatMediaMeta
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRichText
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomOrder
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatSuggestion
import uniffi.truapi.ChatToolbarAction

// Maps the Chat Extension v2 values of the bindings to the chat models of the app, and back. The core
// has bounded and screened every value before the bridge sees it.

fun ChatRoomConfig.toModel() = ChatRoomConfigModel(
    productSource = productSource,
    reactionDetails = reactionDetails,
    participants = participants,
    messageActions = messageActions,
    participantActions = participantActions,
    allowedReactions = allowedReactions,
    placeholder = placeholder,
    reportSeen = reportSeen,
)

fun ChatRoomConfigModel.toCore() = ChatRoomConfig(
    productSource = productSource,
    reactionDetails = reactionDetails,
    participants = participants,
    messageActions = messageActions,
    participantActions = participantActions,
    allowedReactions = allowedReactions,
    placeholder = placeholder,
    reportSeen = reportSeen,
)

fun ChatRoomMetadata.toModel() = ChatRoomMetadataModel(description = description, avatar = avatar?.toModel())

fun ChatRoomPreview.toModel() = ChatRoomPreviewModel(
    order = when (order) {
        ChatRoomOrder.TIMESTAMP -> Order.ByTimestamp
        ChatRoomOrder.PIN_TO_TOP -> Order.PinToTop
    },
    timestamp = timestamp?.toLong(),
    badge = badge?.let {
        when (it) {
            ChatRoomBadge.Hidden -> ChatRoomBadgeModel.Hidden
            is ChatRoomBadge.UnreadCount -> ChatRoomBadgeModel.Count(it.count.toInt())
            is ChatRoomBadge.Image -> ChatRoomBadgeModel.Image(it.resource.toModel())
        }
    },
    text = text?.let { ChatRoomPreviewTextModel(it.text, it.ongoing) },
)

fun ChatResource.toModel(): ChatResourceModel = when (this) {
    is ChatResource.Inline -> ChatResourceModel.Inline(mimeType, bytes.toDataByteArray())
    is ChatResource.Preimage -> ChatResourceModel.Preimage(key.toDataByteArray())
}

/** Null for a v1 address, which no v2 value can carry. */
fun ChatResourceModel.toCore(): ChatResource? = when (this) {
    is ChatResourceModel.Inline -> ChatResource.Inline(mimeType, bytes.value)
    is ChatResourceModel.Preimage -> ChatResource.Preimage(key.value)
    is ChatResourceModel.Url -> null
}

fun ChatToolbarAction.toModel() = ChatToolbarActionModel(
    actionId = actionId,
    label = label,
    icon = icon.toModel(),
    handler = when (val handler = handler) {
        is ChatActionHandler.Deeplink -> ChatActionHandlerModel.Deeplink(handler.url)
        ChatActionHandler.Callback -> ChatActionHandlerModel.Callback
    },
)

fun ChatSuggestion.toModel() = ChatSuggestionModel(suggestionId, text, sendsText)

fun ChatCommandSpec.toModel() = ChatCommandModel(name, description, argumentHint)

fun ChatActivity.toModel() = ChatRoomActivityModel(text, participantId)

fun ChatRoomFooter.toModel(): ChatRoomFooterModel = when (this) {
    is ChatRoomFooter.Input -> ChatRoomFooterModel.Input(
        ChatInputConfigModel(
            placeholder = v1.placeholder,
            photo = v1.photo,
            video = v1.video,
            file = v1.file,
            payment = when (val payment = v1.payment) {
                is ChatInputPayment.Enabled -> ChatInputPaymentModel.Enabled(payment.destination.toDataByteArray())
                null -> null
            },
        ),
    )
    ChatRoomFooter.Empty -> ChatRoomFooterModel.Empty
    ChatRoomFooter.Custom -> ChatRoomFooterModel.Custom
}

fun ChatMediaItem.toModel(): ChatMediaItemModel = when (this) {
    is ChatMediaItem.Image -> ChatMediaItemModel.Image(resource.toModel(), meta.toModel())
    is ChatMediaItem.Video -> ChatMediaItemModel.Video(resource.toModel(), meta.toModel())
    is ChatMediaItem.File -> ChatMediaItemModel.File(resource.toModel(), meta.toModel(), fileName)
}

/** Null when an item has a v1 address. */
fun ChatMediaItemModel.toCore(): ChatMediaItem? {
    val resource = resource.toCore() ?: return null
    val meta = meta.toCore()
    return when (this) {
        is ChatMediaItemModel.Image -> ChatMediaItem.Image(resource, meta)
        is ChatMediaItemModel.Video -> ChatMediaItem.Video(resource, meta)
        is ChatMediaItemModel.File -> ChatMediaItem.File(resource, meta, fileName)
    }
}

private fun ChatMediaMeta.toModel() = ChatMediaMetaModel(
    mimeType = mimeType,
    sizeBytes = sizeBytes?.toLong(),
    width = width?.toInt(),
    height = height?.toInt(),
    placeholder = placeholder,
    alt = alt,
)

private fun ChatMediaMetaModel.toCore() = ChatMediaMeta(
    mimeType = mimeType,
    sizeBytes = sizeBytes?.toULong(),
    width = width?.toUInt(),
    height = height?.toUInt(),
    placeholder = placeholder,
    alt = alt,
)

fun ChatActions.toBody() = ProductMessageBody.Actions(
    text = text,
    actions = actions.map { ChatActionButtonModel(it.actionId, it.title) },
    layout = when (layout) {
        ChatActionLayout.COLUMN -> ChatActionsLayoutModel.COLUMN
        ChatActionLayout.GRID -> ChatActionsLayoutModel.GRID
    },
)

fun ProductMessageBody.Actions.toCore() = ChatActions(
    text = text,
    actions = actions.map { uniffi.truapi.ChatAction(it.actionId, it.title) },
    layout = when (layout) {
        ChatActionsLayoutModel.COLUMN -> ChatActionLayout.COLUMN
        ChatActionsLayoutModel.GRID -> ChatActionLayout.GRID
    },
)

/**
 * The message of a v1 `post_message` body, or null for a reaction, which is no message. v1 media are
 * `https` or `data:` image addresses, and the bridge keeps them as addresses.
 */
fun ChatMessageContent.toProductBotMessage(alt: String?): ProductBotMessage? = when (this) {
    is ChatMessageContent.Text -> ProductBotMessage.Text(text)
    is ChatMessageContent.Custom -> ProductBotMessage.Custom(v1.messageType, v1.payload.toDataByteArray(), alt)
    is ChatMessageContent.RichText -> ProductBotMessage.Body(v1.toBody())
    is ChatMessageContent.Actions -> ProductBotMessage.Body(v1.toBody())
    is ChatMessageContent.File -> ProductBotMessage.Body(v1.toBody())
    is ChatMessageContent.Reaction,
    is ChatMessageContent.ReactionRemoved,
    -> null
}

private fun ChatRichText.toBody() = ProductMessageBody.RichText(
    text = text,
    media = media.map { ChatMediaItemModel.Image(ChatResourceModel.Url(it.url), urlMeta(it.url)) },
)

private fun ChatFile.toBody() = ProductMessageBody.RichText(
    text = text,
    media = listOf(
        ChatMediaItemModel.File(
            resource = ChatResourceModel.Url(url),
            meta = ChatMediaMetaModel(mimeType, sizeBytes.toLong(), width = null, height = null, placeholder = null, alt = null),
            fileName = fileName,
        ),
    ),
)

private fun urlMeta(url: String) = ChatMediaMetaModel(
    mimeType = url.removePrefix("data:").takeIf { url.startsWith("data:") }?.substringBefore(';')?.substringBefore(',') ?: "image/*",
    sizeBytes = null,
    width = null,
    height = null,
    placeholder = null,
    alt = null,
)
