package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.previewText

/**
 * The rich text of a product body, so a product message draws as a message of a person chat. The host does not fetch the
 * media of a body yet, so the message shows the text of the body, or the alt text of its first media item.
 */
fun ChatMessage.Content.ProductBody.toRichText(): ChatMessage.Content.RichText =
    ChatMessage.Content.RichText(text = body.previewText(), attachments = emptyList())
