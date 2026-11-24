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

        /**
         * A tap or pan on a gesture node: [x] and [y] are the translation from the pan's start in dp, clamped to
         * the travel, and [fractionPermille] how far along the travel the child is, 0 to 1000.
         */
        data class Gesture(val phase: JsGesturePhase, val x: Int, val y: Int, val fractionPermille: Int) : Type
    }
}
