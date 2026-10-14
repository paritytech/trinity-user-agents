package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import kotlinx.collections.immutable.ImmutableList

data class ChatRequestsListUiState(
    val requests: ImmutableList<ChatRequestItem>
) {
    val isEmpty: Boolean = requests.isEmpty()

    data class ChatRequestItem(
        val accountId: AccountId,
        val display: ChatDisplayUiModel,
        val timestamp: Long
    ) {
        val uniqueKey: String = accountId.value.toHexString()
    }
}

data class ChatRequestDeclineConfirmationState(
    val isVisible: Boolean,
    val requestItem: ChatRequestsListUiState.ChatRequestItem?
) {
    companion object {
        val Hidden = ChatRequestDeclineConfirmationState(
            isVisible = false,
            requestItem = null
        )
    }
}
