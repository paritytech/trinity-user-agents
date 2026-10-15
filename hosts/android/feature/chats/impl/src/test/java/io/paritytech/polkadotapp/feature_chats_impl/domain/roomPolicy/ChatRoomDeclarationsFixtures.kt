package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewTextModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order

fun declarations(
    config: ChatRoomConfigModel? = ChatRoomConfigModel.HOST_DEFAULT,
    description: String? = null,
    avatar: ChatResourceModel? = null,
    preview: ChatRoomPreviewModel? = null,
    footer: ChatRoomFooterModel? = null,
    features: ChatRoomFeaturesModel? = null,
) = ChatRoomDeclarations(
    config = config,
    configGeneration = 1,
    metadata = ChatRoomMetadataModel(description = description, avatar = avatar),
    preview = preview,
    toolbar = null,
    suggestions = null,
    commands = null,
    footer = footer,
    features = features,
)

fun preview(
    order: Order = Order.ByTimestamp,
    timestamp: Long? = null,
    badge: ChatRoomBadgeModel? = null,
    text: String? = null,
    ongoing: Boolean = false,
) = ChatRoomPreviewModel(
    order = order,
    timestamp = timestamp,
    badge = badge,
    text = text?.let { ChatRoomPreviewTextModel(it, ongoing) },
)
