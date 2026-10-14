package io.paritytech.polkadotapp.feature_chats_impl.utils

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType
import io.paritytech.polkadotapp.common.R as RCommon

fun Attachment.Meta.toAttachmentType(): MessageAttachmentType = when (this) {
    is Attachment.Meta.Image -> MessageAttachmentType.IMAGE
    is Attachment.Meta.Video -> MessageAttachmentType.VIDEO
    is Attachment.Meta.General -> MessageAttachmentType.FILE
}

@StringRes
fun MessageAttachmentType.emojiRes(): Int = when (this) {
    MessageAttachmentType.IMAGE -> RCommon.string.chat_attachment_emoji_image
    MessageAttachmentType.VIDEO -> RCommon.string.chat_attachment_emoji_video
    MessageAttachmentType.FILE -> RCommon.string.chat_attachment_emoji_file
}

@StringRes
fun MessageAttachmentType.nameRes(): Int = when (this) {
    MessageAttachmentType.IMAGE -> RCommon.string.chat_attachment_name_image
    MessageAttachmentType.VIDEO -> RCommon.string.chat_attachment_name_video
    MessageAttachmentType.FILE -> RCommon.string.chat_attachment_name_file
}
