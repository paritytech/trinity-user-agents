package io.paritytech.polkadotapp.feature_chats_api.domain.model

sealed class ChatAction {
    class ButtonClicked(val buttonId: String, val messageId: String) : ChatAction()
}
