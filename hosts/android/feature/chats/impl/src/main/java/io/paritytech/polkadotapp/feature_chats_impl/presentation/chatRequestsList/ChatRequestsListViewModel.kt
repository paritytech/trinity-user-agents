package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithRequestTimestamp
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.asChatRequestError
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatRequestsListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestDeclineConfirmationState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestsListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.error.toPresentationError
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import javax.inject.Inject

@HiltViewModel
class ChatRequestsListViewModel @Inject constructor(
    private val interactor: ChatRequestsListInteractor,
    private val router: ChatsRouter,
    private val chatEngine: ChatEngine
) : BaseViewModel(), ChatRequestsListContract {
    override val state = interactor.subscribeChatRequests()
        .map { contactsWithTimestamp -> contactsWithTimestamp.map { it.toRequestItem() }.toImmutableList() }
        .map { requests -> ChatRequestsListUiState(requests) }
        .withLoading()
        .inBackground()
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = LoadingState.Loading
        )

    override val declineConfirmationState = MutableStateFlow(ChatRequestDeclineConfirmationState.Hidden)

    override fun onRequestClick(request: ChatRequestsListUiState.ChatRequestItem) {
        val payload = ChatFeedPayload.existingContactChat(request.accountId)
        router.openChatFeed(payload)
    }

    override fun onDeclineClick(request: ChatRequestsListUiState.ChatRequestItem) {
        declineConfirmationState.update {
            ChatRequestDeclineConfirmationState(
                isVisible = true,
                requestItem = request
            )
        }
    }

    override fun onConfirmDeclineClick() = launchUnit {
        declineConfirmationState.value.requestItem?.let { request ->
            interactor.declineRequest(request.accountId)
                .onFailure { showPresentationError(it.asChatRequestError().toPresentationError()) }
        }

        declineConfirmationState.update { ChatRequestDeclineConfirmationState.Hidden }
    }

    override fun onCancelDeclineClick() {
        declineConfirmationState.update { ChatRequestDeclineConfirmationState.Hidden }
    }

    override fun onBackClick() {
        router.back()
    }

    private fun ContactWithRequestTimestamp.toRequestItem(): ChatRequestsListUiState.ChatRequestItem {
        return ChatRequestsListUiState.ChatRequestItem(
            accountId = contact.accountId,
            display = chatEngine.getContactChatDisplay(contact).toUi(),
            timestamp = requestTimestamp
        )
    }
}
