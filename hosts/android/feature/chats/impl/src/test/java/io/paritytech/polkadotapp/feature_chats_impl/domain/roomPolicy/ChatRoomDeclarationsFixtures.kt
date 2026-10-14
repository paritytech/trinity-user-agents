package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResource
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadge
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataDeclaration
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomOrder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewText

fun declarations(
    config: ChatRoomConfig = ChatRoomConfig.HostDefault,
    description: String? = null,
    avatar: ChatResource? = null,
    preview: ChatRoomPreview? = null,
    footer: ChatRoomFooterConfig? = null,
) = ChatRoomDeclarations(
    config = config,
    metadata = ChatRoomMetadataDeclaration(description = description, avatar = avatar),
    preview = preview,
    toolbar = emptyList(),
    suggestions = emptyList(),
    commands = emptyList(),
    footer = footer,
)

fun preview(
    order: ChatRoomOrder? = null,
    timestamp: Long? = null,
    badge: ChatRoomBadge? = null,
    text: String? = null,
    ongoing: Boolean = false,
) = ChatRoomPreview(
    order = order,
    timestamp = timestamp,
    badge = badge,
    text = text?.let { ChatRoomPreviewText(it, ongoing) },
)
