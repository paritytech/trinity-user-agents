package io.paritytech.polkadotapp.feature_chats_impl.utils

import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment

fun Attachment.Meta.toEmoji(): String = when (this) {
    is Attachment.Meta.Image -> "🖼️"
    is Attachment.Meta.Video -> "🎥"
    is Attachment.Meta.General -> "📎"
}
