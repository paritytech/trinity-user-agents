package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
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
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class ProductStatusBarViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val productSessionController: ProductSessionController,
    private val interactor: ProductStatusBarInteractor,
    private val router: ProductsRouter,
) : BaseViewModel(), ProductStatusBarContract {
    private val openedFor = savedStateHandle.get<SpaBrowserPayload>(SpaBrowserPayload::class.java.name)

    private val reloads = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    // The browser opens its payload's tab once its own screen composes, after this bar has subscribed.
    private val activeTab: Flow<BrowserSessionInfo?> = productSessionController.activeTab
        .dropWhile { tab -> !isOpenedTab(tab) }

    private val menu: Flow<ProductMenu> = activeTab
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

    override val state: StateFlow<ProductStatusBarUiState?> = combine(activeTab, menu) { tab, menu ->
        ProductStatusBarUiState(title = tab?.title.orEmpty(), menu = menu)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    private var openChatJob: Job? = null

    override fun onProceedInChatClick() {
        val product = state.value?.menu as? ProductMenu.Product ?: return
        if (product.chat != ProductChat.Served || openChatJob?.isActive == true) return

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
        val product = state.value?.menu as? ProductMenu.Product ?: return
        interactor.share(product.id)
    }

    override fun onCloseAppClick() {
        productSessionController.closeActiveTab()
        router.leaveBrowser()
    }

    private suspend fun isOpenedTab(tab: BrowserSessionInfo?): Boolean {
        val openedProduct = when (val payload = openedFor) {
            is SpaBrowserPayload.ByProductId -> payload.productId
            is SpaBrowserPayload.ByUrl -> interactor.productIdOrNull(payload.url)
            null -> null
        } ?: return true

        return tab?.url?.let { interactor.productIdOrNull(it) } == openedProduct
    }

    private suspend fun chatOf(productId: ProductId): ProductChat =
        interactor.includesChat(productId)
            .logFailure("Failed to resolve the product for its menu")
            .fold(
                onSuccess = { served -> if (served) ProductChat.Served else ProductChat.NotServed },
                onFailure = { ProductChat.Unresolved },
            )
}
