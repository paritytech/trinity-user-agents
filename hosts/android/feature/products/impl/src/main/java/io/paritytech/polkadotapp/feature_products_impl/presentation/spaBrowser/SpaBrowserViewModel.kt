package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.product.update.ProductUpdater
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.SpaBrowserInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductMenu
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarUiState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject

/**
 * The browser fragment's presentation: activates the product session for its payload and shows the active
 * tab (attaching its live WebView).
 */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class SpaBrowserViewModel @Inject constructor(
    private val productSessionController: ProductSessionController,
    private val interactor: SpaBrowserInteractor,
    private val productUpdater: ProductUpdater,
    private val router: ProductsRouter,
    savedStateHandle: SavedStateHandle,
) : BaseViewModel(), SpaBrowserContract {
    val webView: StateFlow<WebView?> = productSessionController.webView

    private val openedFor = savedStateHandle.get<SpaBrowserPayload>(SpaBrowserPayload::class.java.name)

    private val closeAppRequested = MutableStateFlow(false)

    private var openChatJob: Job? = null

    // activeTab still names the previous product until the controller opens this payload's tab.
    private val activeTab: Flow<BrowserSessionInfo?> = productSessionController.activeTab
        .dropWhile { tab -> !isOpenedTab(tab) }

    // One load per tab, in two phases: resolve the manifest, then serve the archive. Both report
    // into this stream, so progress and page state are views of it rather than separate truths.
    private val pageLoad: Flow<PageLoad> = activeTab
        .map { tab -> productIdOf(tab?.url) }
        .distinctUntilChanged()
        .flatMapLatest { productId ->
            if (productId == null) {
                flowOf<PageLoad>(PageLoad.NotAProduct)
            } else {
                flow {
                    emit(PageLoad.Resolving(productId))

                    interactor.resolveProduct(productId).fold(
                        onSuccess = { resolved ->
                            emitAll(productSessionController.loadProgress.map { PageLoad.Serving(productId, resolved, it) })
                        },
                        onFailure = { emit(PageLoad.Failed(productId, it.toProductResolutionError())) },
                    )
                }
            }
        }
        .onStart { emit(PageLoad.NotAProduct) }

    private val servedPageName: StateFlow<String?> = pageLoad
        .map { load -> (load as? PageLoad.Serving)?.resolved?.executables?.app?.host?.value }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    override val state: StateFlow<SpaBrowserUiState> = combine(
        activeTab,
        pageLoad,
    ) { info, load ->
        SpaBrowserUiState(
            statusBar = ProductStatusBarUiState(title = info?.title.orEmpty(), menu = load.toMenu()),
            loadProgress = load.toProgress(),
            pageState = load.toPageState(),
            isDevServer = info?.isDevServer == true,
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, SpaBrowserUiState())

    init {
        // With no payload (opened from the tab bar) the active tab is already selected — just make
        // sure its WebView is live.
        when (val payload = openedFor) {
            is SpaBrowserPayload.ByProductId -> productSessionController.openProduct(payload.productId)
            is SpaBrowserPayload.ByUrl -> productSessionController.openUrl(payload.url)
            null -> productSessionController.ensureActiveLive()
        }

        leaveOnceClosed()
    }

    // The visit history spans one stay in the browser: however the screen was left, the next tab
    // opened starts its own back chain.
    override fun onCleared() {
        super.onCleared()

        productSessionController.resetVisitHistory()
    }

    override fun onRefresh() {
        launch {
            val appliedUpdate = servedPageName.value?.let { productUpdater.applyPage(it) } == true
            if (!appliedUpdate) productSessionController.reload()
        }
    }

    // Back unwinds the active tab's page history first, then the tab visit history (tabs stay open), and
    // only leaves the browser once neither has anything left.
    override fun onBackPressed() {
        val wv = webView.value
        when {
            wv != null && wv.canGoBack() -> wv.goBack()
            productSessionController.goToPreviousTab() -> Unit
            else -> router.leaveBrowser()
        }
    }

    override fun onProceedInChatClick() {
        val product = state.value.statusBar?.menu as? ProductMenu.Product ?: return
        if (!product.servesChat || openChatJob?.isActive == true) return

        openChatJob = launch {
            interactor.installChatAndAwaitRoomCreated(product.id)
                .onSuccess(router::openChat)
                .logFailure("Failed to open the product chat")
        }
    }

    override fun onShareClick() {
        val product = state.value.statusBar?.menu as? ProductMenu.Product ?: return
        interactor.share(product.id)
    }

    override fun onCloseAppClick() {
        productSessionController.closeActiveTab()
        closeAppRequested.value = true
    }

    override fun onScreenStarted() {
        launch {
            val productId = productIdOf(activeTab.first()?.url) ?: return@launch
            productUpdater.check(productId).logFailure("Failed to check $productId for an update")
        }
    }

    private fun leaveOnceClosed() {
        launch {
            merge(
                // Wait until at least one tab is there to account for potential races during first tab opening
                productSessionController.openTabs.dropWhile { it.isEmpty() }.filter { it.isEmpty() },
                closeAppRequested.filter { it },
            ).first()

            router.leaveBrowser()
        }
    }

    private suspend fun isOpenedTab(tab: BrowserSessionInfo?): Boolean {
        val openedProduct = when (val payload = openedFor) {
            is SpaBrowserPayload.ByProductId -> payload.productId
            is SpaBrowserPayload.ByUrl -> productIdOf(payload.url)
            null -> null
        } ?: return true

        return productIdOf(tab?.url) == openedProduct
    }

    private suspend fun productIdOf(url: String?): ProductId? = url?.let { interactor.productIdOrNull(it) }
}

private sealed interface PageLoad {
    /** The tab is on an ordinary web page: nothing to resolve, and no archive to fetch. */
    data object NotAProduct : PageLoad
    data class Resolving(val productId: ProductId) : PageLoad
    data class Serving(val productId: ProductId, val resolved: ResolvedProduct, val progress: DotNsLoadProgress) : PageLoad
    data class Failed(val productId: ProductId, val error: ProductResolutionError) : PageLoad
}

private fun PageLoad.toProgress(): DotNsLoadProgress = when (this) {
    PageLoad.NotAProduct -> DotNsLoadProgress.Idle
    is PageLoad.Resolving -> DotNsLoadProgress.Resolving
    is PageLoad.Serving -> progress
    is PageLoad.Failed -> DotNsLoadProgress.Failed(error)
}

private fun PageLoad.toPageState(): SpaBrowserPageState = when (this) {
    // Nothing to show yet is still the product's own page — the progress bar carries the wait.
    PageLoad.NotAProduct, is PageLoad.Resolving -> SpaBrowserPageState.Content
    is PageLoad.Serving -> {
        if (resolved.executables.app != null) SpaBrowserPageState.Content else SpaBrowserPageState.NoAppSurface
    }

    is PageLoad.Failed -> SpaBrowserPageState.Failed(error)
}

private fun PageLoad.toMenu(): ProductMenu = when (this) {
    PageLoad.NotAProduct -> ProductMenu.WebPage
    is PageLoad.Resolving -> ProductMenu.Product(productId, servesChat = false)
    is PageLoad.Serving -> ProductMenu.Product(productId, servesChat = resolved.executables.worker?.includesChat == true)
    is PageLoad.Failed -> ProductMenu.Product(productId, servesChat = false)
}

private fun Throwable.toProductResolutionError(): ProductResolutionError =
    this as? ProductResolutionError ?: ProductResolutionError.Unknown
