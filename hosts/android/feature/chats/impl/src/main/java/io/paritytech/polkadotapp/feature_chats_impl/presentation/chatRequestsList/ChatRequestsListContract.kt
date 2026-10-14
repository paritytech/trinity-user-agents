package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestDeclineConfirmationState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestsListUiState
import kotlinx.coroutines.flow.StateFlow

interface ChatRequestsListContract {
    val state: StateFlow<LoadingState<ChatRequestsListUiState>>
    val declineConfirmationState: StateFlow<ChatRequestDeclineConfirmationState>

    fun onRequestClick(request: ChatRequestsListUiState.ChatRequestItem)
    fun onDeclineClick(request: ChatRequestsListUiState.ChatRequestItem)
    fun onConfirmDeclineClick()
    fun onCancelDeclineClick()

    fun onBackClick()
}
