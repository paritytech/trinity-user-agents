package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

fun declarations(
    config: ChatRoomConfigModel? = ChatRoomConfigModel.HOST_DEFAULT,
    description: String? = null,
    preview: ChatRoomPreviewModel? = null,
    placeholder: String? = null,
) = ChatRoomDeclarations(
    config = config,
    configGeneration = 1,
    metadata = ChatRoomMetadataModel(description = description, placeholder = placeholder),
    preview = preview,
)

fun preview(
    timestamp: Long? = null,
    badge: ChatRoomBadgeModel? = null,
    lastMessage: ProductRoomMessage? = null,
    lastReaction: ChatPreviewReactionModel? = null,
    unseenReactions: Int = 0,
    pinned: Boolean = false,
    activity: ChatRoomActivityModel? = null,
) = ChatRoomPreviewModel(
    timestamp = timestamp,
    badge = badge,
    lastMessage = lastMessage,
    lastReaction = lastReaction,
    unseenReactions = unseenReactions,
    pinned = pinned,
    activity = activity,
)
