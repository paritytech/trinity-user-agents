package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

class JsUiEvent(
    val messageId: ChatMessageId,
    val messageType: String,
    val actionId: String,
    val eventType: Type,
    val roomId: ProductChatIdParameter?,
) {
    sealed interface Type {
        object ButtonClick : Type

        class InputFieldValueChange(val newValue: String) : Type
    }
}
