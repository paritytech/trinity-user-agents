package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu.ProductChatMenuInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class ProductChatMenuViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val interactor: ProductChatMenuInteractor
) : BaseViewModel(), ProductChatMenuContract {
    override val currentPage = MutableStateFlow(ProductChatMenuPage.MAIN)

    override fun onOpenAppClick(productId: ProductId) {
        router.openSpaBrowser(SpaBrowserPayload.ByProductId(productId.value))
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
}
