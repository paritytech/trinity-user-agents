package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
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
