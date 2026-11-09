package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.net.Uri
import android.webkit.WebChromeClient.FileChooserParams
import android.webkit.WebView
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContext
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.WebFileChooserExecutor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.fundingPageOrigin

data class FundingProviderFrameUiState(
    val showsSentFunds: Boolean,
    val isContentVisible: Boolean,
    val failed: Boolean,
)

/** An external https page shown in the frame, and where it finishes. */
data class FundingExternalPage(
    val url: String,
    val returnUrl: String?,
)

/**
 * A provider's own screen, for as long as the provider needs it: a route of its App, or an external https page
 * such as an onramp's widget. The session is already running, so backing out of it closes the screen but leaves the
 * provider waiting; the CASH card opens it again, loading the App route or page afresh. Saying a bank transfer was sent answers `Closed`, and a
 * page reaching its return URL answers `Returned`; the overlay answers `Closed` too once the payment is seen or the
 * session is over.
 */
@HiltViewModel(assistedFactory = FundingProviderFrameViewModel.Factory::class)
class FundingProviderFrameViewModel @AssistedInject constructor(
    /** The frame its sheet found when it opened, which the overlay may have dropped since. */
    @Assisted private val context: FundingFrameContext,
    private val router: ProductsRouter,
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider,
    spaHost: SpaHost,
) : BaseViewModel() {
    /** The external page, drawn in a WebView of the screen's own; `null` for the provider's App. */
    val page: FundingExternalPage? = (context.target as? ProviderFrameTarget.Page)?.let {
        FundingExternalPage(url = it.url, returnUrl = it.returnUrl)
    }

    private val pageOrigin = MutableStateFlow(page?.let { originOf(it.url) })

    /** The origin of the page the external WebView is on, which the frame's chrome names; `null` for the App. */
    val origin: StateFlow<String?> = pageOrigin

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

    /** A main-frame load began, which clears a failed one; the chrome names wherever the page went. */
    fun onPageStarted(url: String) {
        pageOrigin.value = originOf(url)
        pageProgress.value = DotNsLoadProgress.Idle
    }

    /** The main frame is on [url] now, which the chrome names. */
    fun onNavigated(url: String) {
        pageOrigin.value = originOf(url)
    }

    /** WebView finishes a load that failed too, and a failure stays until the next load starts. */
    fun onPageLoaded() {
        if (pageProgress.value !is DotNsLoadProgress.Failed) pageProgress.value = DotNsLoadProgress.Completed
    }

    fun onPageFailed(cause: Throwable) {
        pageProgress.value = DotNsLoadProgress.Failed(cause)
    }

    /**
     * The files the user picked for the page's `<input type="file">`, as a KYC check asks for an ID; `null` when
     * the picker was dismissed or failed, which releases the input so it can open again.
     */
    suspend fun pickFiles(params: FileChooserParams): Array<Uri>? =
        runCatching { WebFileChooserExecutor(contextManager.requireActivity(), params, fileProvider).execute() }
            .flatMap { it }
            .logFailure("Funding page file chooser failed")
            .getOrNull()

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

    /** "ramp.example" or "ramp.example:8443": the core's origin of [url], with its port only when not the default. */
    private fun originOf(url: String): String? =
        fundingPageOrigin(url)?.let { origin -> origin.host + origin.port?.let { ":$it" }.orEmpty() }

    @AssistedFactory
    interface Factory {
        fun create(context: FundingFrameContext): FundingProviderFrameViewModel
    }
}
