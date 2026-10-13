package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.net.Uri
import android.webkit.WebView
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContext
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

data class FundingProviderFrameUiState(
    val showsSentFunds: Boolean,
    val isContentVisible: Boolean,
    val failed: Boolean,
)

/** An external https page shown in the frame, with the origin the frame's chrome names and where it finishes. */
data class FundingExternalPage(
    val url: String,
    val origin: String,
    val returnUrl: String?,
)

/**
 * A provider's own screen, for as long as the provider needs it: a route of its App, or an external https page
 * such as an onramp's widget. The session is already running, so backing out of it only hides it and leaves the
 * provider waiting, which the CASH card can show again. Saying a bank transfer was sent answers `Closed`, and a
 * page reaching its return URL answers `Returned`; the overlay answers `Closed` too once the payment is seen or the
 * session is over.
 */
@HiltViewModel
class FundingProviderFrameViewModel @Inject constructor(
    private val context: FundingFrameContext,
    private val router: ProductsRouter,
    spaHost: SpaHost,
) : BaseViewModel() {
    /** The external page, drawn in a WebView of the screen's own; `null` for the provider's App. */
    val page: FundingExternalPage? = (context.target as? ProviderFrameTarget.Page)?.let {
        FundingExternalPage(url = it.url, origin = originOf(it.url), returnUrl = it.returnUrl)
    }

    private val session = if (page == null) spaHost.createSession(context.url) else null
    private val pageProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)

    val webView: StateFlow<WebView?> = session?.webView ?: MutableStateFlow(null)

    val state: StateFlow<FundingProviderFrameUiState> = (session?.loadProgress ?: pageProgress)
        .scan(initialState()) { previous, progress ->
            previous.copy(
                isContentVisible = previous.isContentVisible || progress == DotNsLoadProgress.Completed,
                failed = progress is DotNsLoadProgress.Failed,
            )
        }
        .stateIn(this, SharingStarted.Eagerly, initialState())

    init {
        context.isShown = true
    }

    fun onBack() {
        val current = webView.value
        if (current != null && current.canGoBack()) current.goBack() else hide()
    }

    /** Leaves the frame without answering, as backing out of it does. */
    fun onHide() = hide()

    fun onSentFunds() = finish(ProviderFrameOutcome.Closed)

    fun onPageLoaded() {
        pageProgress.value = DotNsLoadProgress.Completed
    }

    fun onPageFailed(cause: Throwable) {
        pageProgress.value = DotNsLoadProgress.Failed(cause)
    }

    /** The page reached its return URL; [url] carries the result in its query. */
    fun onPageReturned(url: String) = finish(ProviderFrameOutcome.Returned(url))

    fun pauseConnections() {
        session?.pauseConnections()
    }

    fun resumeConnections() {
        session?.resumeConnections()
    }

    override fun onCleared() {
        super.onCleared()
        context.isShown = false
    }

    private fun hide() = launchUnit { router.closeFundingProviderFrame(context.intent) }

    private fun finish(outcome: ProviderFrameOutcome) = launchUnit {
        context.answer(outcome)
        router.closeFundingProviderFrame(context.intent)
    }

    private fun initialState() = FundingProviderFrameUiState(
        showsSentFunds = context.showsSentFunds,
        isContentVisible = false,
        failed = false,
    )

    private fun originOf(url: String): String {
        val uri = Uri.parse(url)
        val port = uri.port.takeIf { it != -1 }?.let { ":$it" }.orEmpty()
        return "${uri.host.orEmpty()}$port"
    }
}
