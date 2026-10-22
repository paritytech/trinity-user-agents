package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

fun declarations(
    config: ChatRoomConfigModel? = ChatRoomConfigModel.HOST_DEFAULT,
    description: String? = null,
    preview: ChatRoomPreviewModel? = null,
    features: ChatRoomFeaturesModel? = null,
    contact: ChatRoomContactModel? = null,
) = ChatRoomDeclarations(
    config = config,
    configGeneration = 1,
    metadata = ChatRoomMetadataModel(description = description),
    preview = preview,
    features = features,
    contact = contact,
)

fun preview(
    timestamp: Long? = null,
    badge: ChatRoomBadgeModel? = null,
    lastMessage: ProductRoomMessage? = null,
    lastReaction: ChatPreviewReactionModel? = null,
    unseenReactions: Int = 0,
) = ChatRoomPreviewModel(
    timestamp = timestamp,
    badge = badge,
    lastMessage = lastMessage,
    lastReaction = lastReaction,
    unseenReactions = unseenReactions,
)
