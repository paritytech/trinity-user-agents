package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import uniffi.truapi.ChatMediaItem
import uniffi.truapi.ChatMediaMeta
import uniffi.truapi.ChatResource
import uniffi.truapi.ChatRoomBadge
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomPreview

// Maps the Chat Extension v2 values of the bindings to the chat models of the app, and back. The core
// has bounded and screened every value before the bridge sees it.

fun ChatRoomConfig.toModel() = ChatRoomConfigModel(
    productSource = productSource,
    reactionDetails = reactionDetails,
    reportSeen = reportSeen,
)

fun ChatRoomConfigModel.toCore() = ChatRoomConfig(
    productSource = productSource,
    reactionDetails = reactionDetails,
    reportSeen = reportSeen,
)

fun ChatRoomMetadata.toModel() = ChatRoomMetadataModel(description = description)

fun ChatRoomPreview.toModel() = ChatRoomPreviewModel(
    timestamp = timestamp?.toLong(),
    badge = badge?.let {
        when (it) {
            ChatRoomBadge.Hidden -> ChatRoomBadgeModel.Hidden
            is ChatRoomBadge.UnreadCount -> ChatRoomBadgeModel.Count(it.count.toInt())
        }
    },
    lastMessage = lastMessage?.toModel(),
    lastReaction = lastReaction?.let { ChatPreviewReactionModel(it.emoji, it.byUser, it.message?.toModel(), it.removed) },
    unseenReactions = unseenReactions.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt(),
)

fun ChatResource.toModel(): ChatResourceModel = when (this) {
    is ChatResource.Hop -> ChatResourceModel.Hop(identifier.toDataByteArray(), ticket.toDataByteArray(), node, size.toLong())
}

fun ChatResourceModel.toCore(): ChatResource = when (this) {
    is ChatResourceModel.Hop -> ChatResource.Hop(identifier.value, ticket.value, node, size.toULong())
}

fun ChatMediaItem.toModel(): ChatMediaItemModel = when (this) {
    is ChatMediaItem.Image -> ChatMediaItemModel.Image(resource.toModel(), meta.toModel())
    is ChatMediaItem.Video -> ChatMediaItemModel.Video(resource.toModel(), meta.toModel())
    is ChatMediaItem.File -> ChatMediaItemModel.File(resource.toModel(), meta.toModel(), fileName)
}

fun ChatMediaItemModel.toCore(): ChatMediaItem {
    val resource = resource.toCore()
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
    durationMs = durationMs?.toLong(),
)

private fun ChatMediaMetaModel.toCore() = ChatMediaMeta(
    mimeType = mimeType,
    sizeBytes = sizeBytes?.toULong(),
    width = width?.toUInt(),
    height = height?.toUInt(),
    placeholder = placeholder,
    alt = alt,
    durationMs = durationMs?.toULong(),
)
