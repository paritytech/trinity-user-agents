package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionButtonModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionsLayoutModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody

fun ProductMessageBody.toLocal(): ProductMessageBodyLocal = when (this) {
    is ProductMessageBody.RichText -> ProductMessageBodyLocal.RichText(text, media.map { it.toLocal() })
    is ProductMessageBody.Actions -> ProductMessageBodyLocal.Actions(
        text = text,
        actions = actions.map { ChatActionButtonLocal(it.actionId, it.title) },
        layout = when (layout) {
            ChatActionsLayoutModel.COLUMN -> ChatActionsLayoutLocal.COLUMN
            ChatActionsLayoutModel.GRID -> ChatActionsLayoutLocal.GRID
        },
    )
}

fun ProductMessageBodyLocal.toDomain(): ProductMessageBody = when (this) {
    is ProductMessageBodyLocal.RichText -> ProductMessageBody.RichText(text, media.map { it.toDomain() })
    is ProductMessageBodyLocal.Actions -> ProductMessageBody.Actions(
        text = text,
        actions = actions.map { ChatActionButtonModel(it.actionId, it.title) },
        layout = when (layout) {
            ChatActionsLayoutLocal.COLUMN -> ChatActionsLayoutModel.COLUMN
            ChatActionsLayoutLocal.GRID -> ChatActionsLayoutModel.GRID
        },
    )
}

fun ChatResourceModel.toLocal(): ChatResourceLocal = when (this) {
    is ChatResourceModel.Inline -> ChatResourceLocal.Inline(mimeType, bytes.value)
    is ChatResourceModel.Preimage -> ChatResourceLocal.Preimage(key.value)
    is ChatResourceModel.Url -> ChatResourceLocal.Url(url)
    is ChatResourceModel.Sealed -> toLocal()
}

fun ChatResourceModel.Sealed.toLocal() = ChatResourceLocal.Sealed(parts.map { it.value }, key.value, nonce.value, size, digest.value)

fun ChatResourceLocal.Sealed.toDomain() = ChatResourceModel.Sealed(
    parts = parts.map { it.toDataByteArray() },
    key = key.toDataByteArray(),
    nonce = nonce.toDataByteArray(),
    size = size,
    digest = digest.toDataByteArray(),
)

fun ChatResourceLocal.toDomain(): ChatResourceModel = when (this) {
    is ChatResourceLocal.Inline -> ChatResourceModel.Inline(mimeType, bytes.toDataByteArray())
    is ChatResourceLocal.Preimage -> ChatResourceModel.Preimage(key.toDataByteArray())
    is ChatResourceLocal.Url -> ChatResourceModel.Url(url)
    is ChatResourceLocal.Sealed -> toDomain()
}

private fun ChatMediaItemModel.toLocal(): ChatMediaItemLocal = when (this) {
    is ChatMediaItemModel.Image -> ChatMediaItemLocal.Image(resource.toLocal(), meta.toLocal())
    is ChatMediaItemModel.Video -> ChatMediaItemLocal.Video(resource.toLocal(), meta.toLocal())
    is ChatMediaItemModel.File -> ChatMediaItemLocal.File(resource.toLocal(), meta.toLocal(), fileName)
}

private fun ChatMediaItemLocal.toDomain(): ChatMediaItemModel = when (this) {
    is ChatMediaItemLocal.Image -> ChatMediaItemModel.Image(resource.toDomain(), meta.toDomain())
    is ChatMediaItemLocal.Video -> ChatMediaItemModel.Video(resource.toDomain(), meta.toDomain())
    is ChatMediaItemLocal.File -> ChatMediaItemModel.File(resource.toDomain(), meta.toDomain(), fileName)
}

private fun ChatMediaMetaModel.toLocal() = ChatMediaMetaLocal(mimeType, sizeBytes, width, height, placeholder, alt)

private fun ChatMediaMetaLocal.toDomain() = ChatMediaMetaModel(mimeType, sizeBytes, width, height, placeholder, alt)
