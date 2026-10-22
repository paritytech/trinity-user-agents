package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody

fun ProductMessageBody.toLocal(): ProductMessageBodyLocal = when (this) {
    is ProductMessageBody.RichText -> ProductMessageBodyLocal.RichText(text, media.map { it.toLocal() })
}

// A stored body of a kind that Chat Extension v2 no longer has keeps its text only.
fun ProductMessageBodyLocal.toDomain(): ProductMessageBody = when (this) {
    is ProductMessageBodyLocal.RichText -> ProductMessageBody.RichText(text, media.mapNotNull { it.toDomain() })
    is ProductMessageBodyLocal.Actions -> ProductMessageBody.RichText(text, emptyList())
}

fun ChatResourceModel.toLocal(): ChatResourceLocal = when (this) {
    is ChatResourceModel.Url -> ChatResourceLocal.Url(url)
    is ChatResourceModel.Hop -> toLocal()
}

fun ChatResourceModel.Hop.toLocal() = ChatResourceLocal.Hop(identifier.value, ticket.value, node, size)

fun ChatResourceLocal.Hop.toDomain() = ChatResourceModel.Hop(identifier.toDataByteArray(), ticket.toDataByteArray(), node, size)

/** Null for a resource of a kind that Chat Extension v2 no longer has. */
fun ChatResourceLocal.toDomain(): ChatResourceModel? = when (this) {
    is ChatResourceLocal.Url -> ChatResourceModel.Url(url)
    is ChatResourceLocal.Hop -> toDomain()
    is ChatResourceLocal.Inline,
    is ChatResourceLocal.Preimage,
    is ChatResourceLocal.Sealed,
    -> null
}

private fun ChatMediaItemModel.toLocal(): ChatMediaItemLocal = when (this) {
    is ChatMediaItemModel.Image -> ChatMediaItemLocal.Image(resource.toLocal(), meta.toLocal())
    is ChatMediaItemModel.Video -> ChatMediaItemLocal.Video(resource.toLocal(), meta.toLocal())
    is ChatMediaItemModel.File -> ChatMediaItemLocal.File(resource.toLocal(), meta.toLocal(), fileName)
}

private fun ChatMediaItemLocal.toDomain(): ChatMediaItemModel? = when (this) {
    is ChatMediaItemLocal.Image -> resource.toDomain()?.let { ChatMediaItemModel.Image(it, meta.toDomain()) }
    is ChatMediaItemLocal.Video -> resource.toDomain()?.let { ChatMediaItemModel.Video(it, meta.toDomain()) }
    is ChatMediaItemLocal.File -> resource.toDomain()?.let { ChatMediaItemModel.File(it, meta.toDomain(), fileName) }
}

private fun ChatMediaMetaModel.toLocal() = ChatMediaMetaLocal(mimeType, sizeBytes, width, height, placeholder, alt)

private fun ChatMediaMetaLocal.toDomain() = ChatMediaMetaModel(mimeType, sizeBytes, width, height, placeholder, alt)
