package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.model.ChatRoomDeclarationsLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatInputPaymentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewTextModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatResourceLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.toLocal
import kotlinx.serialization.encodeToByteArray
import timber.log.Timber

fun ChatRoomDeclarationsLocal.toDomain(): ChatRoomDeclarations = ChatRoomDeclarations(
    config = config?.decodeOrNull<ChatRoomConfigLocal>("config")?.toDomain(),
    configGeneration = configGeneration,
    metadata = ChatRoomMetadataModel(
        description = description,
        avatar = avatar?.decodeOrNull<ChatResourceLocal>("avatar")?.toDomain(),
    ),
    preview = preview?.decodeOrNull<ChatRoomPreviewLocal>("preview")?.toDomain(),
    toolbar = toolbar?.decodeOrNull<List<ChatToolbarActionLocal>>("toolbar")?.map { it.toDomain() },
    suggestions = suggestions?.decodeOrNull<List<ChatSuggestionLocal>>("suggestions")?.map { it.toDomain() },
    commands = commands?.decodeOrNull<List<ChatCommandLocal>>("commands")?.map { it.toDomain() },
    footer = footer?.decodeOrNull<ChatRoomFooterLocal>("footer")?.toDomain(),
)

fun ChatRoomConfigModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun ChatRoomPreviewModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun ChatRoomFooterModel.encode(): ByteArray = BinaryScale.encodeToByteArray(toLocal())

fun List<ChatToolbarActionModel>.encodeToolbar(): ByteArray = BinaryScale.encodeToByteArray(map { it.toLocal() })

fun List<ChatSuggestionModel>.encodeSuggestions(): ByteArray = BinaryScale.encodeToByteArray(map { it.toLocal() })

fun List<ChatCommandModel>.encodeCommands(): ByteArray = BinaryScale.encodeToByteArray(map { it.toLocal() })

fun ChatRoomMetadataModel.encodeAvatar(): ByteArray? = avatar?.let { BinaryScale.encodeToByteArray(it.toLocal()) }

// A value that does not decode reads as "not declared", so the host default applies until the product declares again.
private inline fun <reified T> ByteArray.decodeOrNull(part: String): T? =
    runCatching { BinaryScale.decodeFromByteArray<T>(this) }
        .onFailure { Timber.w(it, "Cannot decode the %s of a chat room", part) }
        .getOrNull()

fun ChatRoomConfigModel.toLocal() = ChatRoomConfigLocal(
    productSource = productSource,
    reactionDetails = reactionDetails,
    participants = participants,
    messageActions = messageActions,
    participantActions = participantActions,
    allowedReactions = allowedReactions,
    placeholder = placeholder,
    reportSeen = reportSeen,
)

fun ChatRoomConfigLocal.toDomain() = ChatRoomConfigModel(
    productSource = productSource,
    reactionDetails = reactionDetails,
    participants = participants,
    messageActions = messageActions,
    participantActions = participantActions,
    allowedReactions = allowedReactions,
    placeholder = placeholder,
    reportSeen = reportSeen,
)

fun ChatRoomPreviewModel.toLocal() = ChatRoomPreviewLocal(
    order = when (order) {
        Order.PinToTop -> ChatRoomOrderLocal.PIN_TO_TOP
        Order.ByTimestamp -> ChatRoomOrderLocal.TIMESTAMP
    },
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            ChatRoomBadgeModel.Hidden -> ChatRoomBadgeLocal.Hidden
            is ChatRoomBadgeModel.Count -> ChatRoomBadgeLocal.UnreadCount(it.count)
            is ChatRoomBadgeModel.Image -> ChatRoomBadgeLocal.Image(it.resource.toLocal())
        }
    },
    text = text?.let { ChatRoomPreviewTextLocal(it.text, it.ongoing) },
)

fun ChatRoomPreviewLocal.toDomain() = ChatRoomPreviewModel(
    order = when (order) {
        ChatRoomOrderLocal.PIN_TO_TOP -> Order.PinToTop
        ChatRoomOrderLocal.TIMESTAMP -> Order.ByTimestamp
    },
    timestamp = timestamp,
    badge = badge?.let {
        when (it) {
            ChatRoomBadgeLocal.Hidden -> ChatRoomBadgeModel.Hidden
            is ChatRoomBadgeLocal.UnreadCount -> ChatRoomBadgeModel.Count(it.count)
            is ChatRoomBadgeLocal.Image -> ChatRoomBadgeModel.Image(it.resource.toDomain())
        }
    },
    text = text?.let { ChatRoomPreviewTextModel(it.text, it.ongoing) },
)

fun ChatToolbarActionModel.toLocal() = ChatToolbarActionLocal(actionId, label, icon.toLocal(), handler.toLocal())

fun ChatToolbarActionLocal.toDomain() = ChatToolbarActionModel(actionId, label, icon.toDomain(), handler.toDomain())

private fun ChatActionHandlerModel.toLocal(): ChatActionHandlerLocal = when (this) {
    is ChatActionHandlerModel.Deeplink -> ChatActionHandlerLocal.Deeplink(url)
    ChatActionHandlerModel.Callback -> ChatActionHandlerLocal.Callback
}

private fun ChatActionHandlerLocal.toDomain(): ChatActionHandlerModel = when (this) {
    is ChatActionHandlerLocal.Deeplink -> ChatActionHandlerModel.Deeplink(url)
    ChatActionHandlerLocal.Callback -> ChatActionHandlerModel.Callback
}

fun ChatSuggestionModel.toLocal() = ChatSuggestionLocal(suggestionId, text, sendsText)

fun ChatSuggestionLocal.toDomain() = ChatSuggestionModel(suggestionId, text, sendsText)

fun ChatCommandModel.toLocal() = ChatCommandLocal(name, description, argumentHint)

fun ChatCommandLocal.toDomain() = ChatCommandModel(name, description, argumentHint)

fun ChatRoomFooterModel.toLocal(): ChatRoomFooterLocal = when (this) {
    is ChatRoomFooterModel.Input -> ChatRoomFooterLocal.Input(
        ChatInputConfigLocal(
            placeholder = config.placeholder,
            photo = config.photo,
            video = config.video,
            file = config.file,
            payment = config.payment?.let { payment ->
                when (payment) {
                    is ChatInputPaymentModel.Enabled -> ChatInputPaymentLocal.Enabled(payment.destination.value)
                }
            },
        ),
    )
    ChatRoomFooterModel.Empty -> ChatRoomFooterLocal.Empty
    ChatRoomFooterModel.Custom -> ChatRoomFooterLocal.Custom
}

fun ChatRoomFooterLocal.toDomain(): ChatRoomFooterModel = when (this) {
    is ChatRoomFooterLocal.Input -> ChatRoomFooterModel.Input(
        ChatInputConfigModel(
            placeholder = config.placeholder,
            photo = config.photo,
            video = config.video,
            file = config.file,
            payment = config.payment?.let { payment ->
                when (payment) {
                    is ChatInputPaymentLocal.Enabled -> ChatInputPaymentModel.Enabled(payment.destination.toDataByteArray())
                }
            },
        ),
    )
    ChatRoomFooterLocal.Empty -> ChatRoomFooterModel.Empty
    ChatRoomFooterLocal.Custom -> ChatRoomFooterModel.Custom
}
