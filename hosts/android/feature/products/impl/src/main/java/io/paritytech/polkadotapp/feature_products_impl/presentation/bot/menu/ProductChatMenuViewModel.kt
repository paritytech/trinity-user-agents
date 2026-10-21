package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateService
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu.ProductChatMenuInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class ProductChatMenuViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val interactor: ProductChatMenuInteractor,
    private val userStateRepository: ChatRoomUserStateRepository,
    private val userStateService: ChatRoomUserStateService,
) : BaseViewModel(), ProductChatMenuContract {
    override val currentPage = MutableStateFlow(ProductChatMenuPage.MAIN)

    override fun userState(chatId: ChatId): Flow<ChatRoomUserStateModel> = userStateRepository.observe(chatId)

    override fun onOpenAppClick(productId: ProductId) {
        router.openSpaBrowser(SpaBrowserPayload.ByProductId(productId.value))
    }

    override fun onMuteClick(chatId: ChatId, isMuted: Boolean) {
        if (isMuted) {
            changeUserState { userStateService.unmute(chatId) }
        } else {
            currentPage.value = ProductChatMenuPage.MUTE_DURATION
        }
    }

    override fun onMuteDurationSelected(chatId: ChatId, duration: ChatMuteDuration) {
        currentPage.value = ProductChatMenuPage.MAIN
        changeUserState { userStateService.mute(chatId, duration) }
    }

    override fun onArchiveClick(chatId: ChatId, isArchived: Boolean) {
        changeUserState { userStateService.setArchived(chatId, archived = !isArchived) }
    }

    override fun onRemoveChatClick() {
        currentPage.value = ProductChatMenuPage.REMOVE_CONFIRMATION
    }

    override fun onRemoveChatConfirmed(productId: ProductId, chatId: ChatId) = launchUnit {
        interactor.removeChat(productId, chatId)
        router.back()
    }

    override fun onMenuDismissed() {
        currentPage.value = ProductChatMenuPage.MAIN
    }

    private fun changeUserState(change: suspend () -> Result<Unit>) = launchUnit {
        change().onFailure { showPresentationError(UnexpectedPresentationError(it)) }
    }
}
