package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.webkit.WebView
import androidx.core.net.toUri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.SpaBrowserInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
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
    private val router: ProductsRouter,
    private val dotNsTldProvider: DotNsTldProvider,
    savedStateHandle: SavedStateHandle,
) : BaseViewModel(), SpaBrowserContract {
    val webView: StateFlow<WebView?> = productSessionController.webView

    // One load per tab, in two phases: resolve the manifest, then serve the archive. Both report
    // into this stream, so progress and page state are views of it rather than separate truths.
    private val pageLoad: Flow<PageLoad> = productSessionController.activeTab
        .map { tab ->
            // Waits rather than guesses: a tab classified before the TLD lands would stick as
            // "not a product" for its whole lifetime.
            tab?.url?.toUri()?.let { ProductId.fromUrl(it, dotNsTldProvider.getTldRetrying()).getOrNull() }
        }
        .distinctUntilChanged()
        .flatMapLatest { productId ->
            if (productId == null) {
                flowOf<PageLoad>(PageLoad.NotAProduct)
            } else {
                flow {
                    emit(PageLoad.Resolving)

                    interactor.resolveProduct(productId).fold(
                        onSuccess = { resolved ->
                            emitAll(productSessionController.loadProgress.map { PageLoad.Serving(resolved, it) })
                        },
                        onFailure = { emit(PageLoad.Failed(it.toProductResolutionError())) },
                    )
                }
            }
        }

    override val state: StateFlow<SpaBrowserUiState> = combine(
        productSessionController.activeTab,
        pageLoad,
    ) { info, load ->
        SpaBrowserUiState(
            title = info?.title,
            subtitle = info?.host,
            loadProgress = load.toProgress(),
            pageState = load.toPageState(),
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, SpaBrowserUiState())

    init {
        // With no payload (opened from the tab bar) the active tab is already selected — just make
        // sure its WebView is live.
        when (val payload = savedStateHandle.get<SpaBrowserPayload>(SpaBrowserPayload::class.java.name)) {
            is SpaBrowserPayload.ByProductId -> productSessionController.openProduct(ProductId.fromStoredValue(payload.productId))
            is SpaBrowserPayload.ByUrl -> productSessionController.openUrl(payload.url)
            null -> productSessionController.ensureActiveLive()
        }

        quitOnLastTabClosed()
    }

    // The visit history spans one stay in the browser: however the screen was left, the next tab
    // opened starts its own back chain.
    override fun onCleared() {
        super.onCleared()

        productSessionController.resetVisitHistory()
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

    private fun quitOnLastTabClosed() {
        launch {
            productSessionController.openTabs
                // Wait until at least one tab is there to account for potential races during first tab opening
                .dropWhile { it.isEmpty() }
                .first { it.isEmpty() }

            router.leaveBrowser()
        }
    }
}

private sealed interface PageLoad {
    /** The tab is on an ordinary web page: nothing to resolve, and no archive to fetch. */
    data object NotAProduct : PageLoad
    data object Resolving : PageLoad
    data class Serving(val resolved: ResolvedProduct, val progress: DotNsLoadProgress) : PageLoad
    data class Failed(val error: ProductResolutionError) : PageLoad
}

private fun PageLoad.toProgress(): DotNsLoadProgress = when (this) {
    PageLoad.NotAProduct -> DotNsLoadProgress.Idle
    PageLoad.Resolving -> DotNsLoadProgress.Resolving
    is PageLoad.Serving -> progress
    is PageLoad.Failed -> DotNsLoadProgress.Failed(error)
}

private fun PageLoad.toPageState(): SpaBrowserPageState = when (this) {
    // Nothing to show yet is still the product's own page — the progress bar carries the wait.
    PageLoad.NotAProduct, PageLoad.Resolving -> SpaBrowserPageState.Content
    is PageLoad.Serving -> {
        if (resolved.executables.app != null) SpaBrowserPageState.Content else SpaBrowserPageState.NoAppSurface
    }

    is PageLoad.Failed -> SpaBrowserPageState.Failed(error)
}

private fun Throwable.toProductResolutionError(): ProductResolutionError =
    this as? ProductResolutionError ?: ProductResolutionError.Unknown
