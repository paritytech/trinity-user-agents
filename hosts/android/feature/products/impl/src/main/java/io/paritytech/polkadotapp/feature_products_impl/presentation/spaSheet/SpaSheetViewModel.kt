package io.paritytech.polkadotapp.feature_products_impl.presentation.spaSheet

import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaSheetPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

/**
 * Hosts a single product in a sheet, with none of the browser's tab machinery: the session lives
 * and dies with this ViewModel, so leaving the sheet tears the WebView down.
 */
@HiltViewModel
class SpaSheetViewModel @Inject constructor(
    spaHost: SpaHost,
    private val router: ProductsRouter,
    savedStateHandle: SavedStateHandle,
) : BaseViewModel(), SpaSheetContract {
    private val payload = savedStateHandle.getPayload<SpaSheetPayload>()

    private val session = spaHost.createSession(Urls.ensureHasProtocolOrHttps(payload.url))

    val webView: StateFlow<WebView?> = session.webView

    override val state: StateFlow<SpaSheetUiState> = session.loadProgress
        .scan(SpaSheetUiState()) { previous, progress ->
            SpaSheetUiState(
                loadProgress = progress,
                // Once the first archive is served the product owns the sheet — navigations it makes
                // afterwards must not blank it back out.
                isContentVisible = previous.isContentVisible || progress == DotNsLoadProgress.Completed,
            )
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, SpaSheetUiState())

    // The product feels native, so back unwinds its own history first and only leaves once it has none.
    override fun onBackPressed() {
        val currentWebView = webView.value
        if (currentWebView != null && currentWebView.canGoBack()) {
            currentWebView.goBack()
        } else {
            router.back()
        }
    }

    fun pauseConnections() {
        session.pauseConnections()
    }

    fun resumeConnections() {
        session.resumeConnections()
    }
}
