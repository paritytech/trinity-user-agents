package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.statusBar.ProductStatusBarInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ProductStatusBarViewModel @Inject constructor(
    private val productSessionController: ProductSessionController,
    private val interactor: ProductStatusBarInteractor,
    private val router: ProductsRouter,
) : BaseViewModel(), ProductStatusBarContract {
    private val reloads = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    private val menu: Flow<ProductMenu> = productSessionController.activeTab
        .map { it?.url }
        .distinctUntilChanged()
        .mapLatest { url -> url?.let { interactor.productIdOrNull(it) } }
        .distinctUntilChanged()
        .flatMapLatest { productId ->
            if (productId == null) {
                flowOf<ProductMenu>(ProductMenu.WebPage)
            } else {
                reloads.onStart { emit(Unit) }
                    .mapLatest<Unit, ProductMenu> { ProductMenu.Product(productId, chatOf(productId)) }
                    .onStart { emit(ProductMenu.Product(productId, ProductChat.Unresolved)) }
            }
        }
        .onStart { emit(ProductMenu.WebPage) }

    override val state: StateFlow<ProductStatusBarUiState> = combine(
        productSessionController.activeTab,
        menu,
    ) { tab, menu ->
        ProductStatusBarUiState(title = tab?.title.orEmpty(), menu = menu)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, ProductStatusBarUiState(title = "", menu = ProductMenu.WebPage))

    private var openChatJob: Job? = null

    override fun onProceedInChatClick() {
        val product = state.value.menu as? ProductMenu.Product ?: return
        if (openChatJob?.isActive == true) return

        openChatJob = launch {
            interactor.installChatAndAwaitRoomCreated(product.id)
                .onSuccess(router::openChat)
                .logFailure("Failed to open the product chat")
        }
    }

    override fun onReloadClick() {
        productSessionController.reload()
        reloads.tryEmit(Unit)
    }

    override fun onShareClick() {
        val product = state.value.menu as? ProductMenu.Product ?: return
        interactor.share(product.id)
    }

    override fun onCloseAppClick() {
        productSessionController.closeActiveTab()
        router.leaveBrowser()
    }

    private suspend fun chatOf(productId: ProductId): ProductChat =
        interactor.includesChat(productId)
            .logFailure("Failed to resolve the product for its menu")
            .fold(
                onSuccess = { served -> if (served) ProductChat.Served else ProductChat.NotServed },
                onFailure = { ProductChat.Unresolved },
            )
}
