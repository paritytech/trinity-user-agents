package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.net.Uri
import android.webkit.WebChromeClient.FileChooserParams
import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.WebFileChooserExecutor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.FundingFrameOutcome
import javax.inject.Inject

data class FundingProviderFrameUiState(
    /** The session waits on a transfer the user makes outside the frame, as its progress says now. */
    val showsSentFunds: Boolean,
    val isContentVisible: Boolean,
    val failed: Boolean,
)

/**
 * A provider's own screen, for as long as the provider needs it: a route of its App, or an external https page such
 * as an onramp's widget. What it shows lives with the [FundingFrame], so backing out of the screen keeps the provider's
 * page where it was; the CASH card shows it again. Saying a bank transfer was sent answers `Closed`, and a page
 * reaching its return URL answers `Returned`.
 */
@HiltViewModel
class FundingProviderFrameViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val frames: FundingFrames,
    private val service: FundingService,
    private val router: ProductsRouter,
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider,
) : BaseViewModel() {
    private val frame: FundingFrame? = frames.get(savedStateHandle.get<String>(FundingProviderFrameBottomSheet.REQUEST).orEmpty())

    /** No frame waits under this screen, as after process death: it closes at once. */
    val isOrphaned: Boolean = frame == null

    val page: FundingExternalPage? = frame?.page

    /** Nothing waits on this screen any more: it was answered, or the provider closed it, while another sat over it. */
    val isAnswered: Boolean
        get() = frame == null || frames.get(frame.request.id) == null

    /** The external page's WebView, which outlives this screen; `null` for the provider's App. */
    val pageView: WebView? = frame?.pageView()

    /** The origin of the page the external WebView is on, which the frame's chrome names; `null` for the App. */
    val origin: StateFlow<String?> = frame?.pageOrigin ?: MutableStateFlow(null)

    val webView: StateFlow<WebView?> = frame?.appWebView ?: MutableStateFlow(null)

    private val awaitsUserTransfer = frame?.let { frame ->
        service.entryState(frame.request.entryId).map { it.progress?.awaitsUserTransfer == true }.onStart { emit(false) }
    } ?: flowOf(false)

    val state: StateFlow<FundingProviderFrameUiState> = (frame?.loadProgress ?: MutableStateFlow(DotNsLoadProgress.Idle))
        .scan(LoadState(isContentVisible = false, failed = false)) { previous, progress ->
            LoadState(
                isContentVisible = previous.isContentVisible || progress == DotNsLoadProgress.Completed,
                failed = progress is DotNsLoadProgress.Failed,
            )
        }
        .combine(awaitsUserTransfer.distinctUntilChanged()) { load, awaits ->
            FundingProviderFrameUiState(showsSentFunds = awaits, isContentVisible = load.isContentVisible, failed = load.failed)
        }
        .stateIn(this, SharingStarted.Eagerly, FundingProviderFrameUiState(showsSentFunds = false, isContentVisible = false, failed = false))

    init {
        frame?.isShown = true
    }

    fun onBack() {
        val current = webView.value
        if (current != null && current.canGoBack()) current.goBack() else hide()
    }

    /** Leaves the frame without answering, as backing out of it does. */
    fun onHide() = hide()

    fun onSentFunds() = finish(FundingFrameOutcome.Closed)

    /** A main-frame load began, which clears a failed one; the chrome names wherever the page went. */
    fun onPageStarted(url: String) {
        frame?.pageOrigin?.value = originOf(url)
        frame?.pageProgress?.value = DotNsLoadProgress.Idle
    }

    /** The main frame is on [url] now, which the chrome names. */
    fun onNavigated(url: String) {
        frame?.pageOrigin?.value = originOf(url)
    }

    /** WebView finishes a load that failed too, and a failure stays until the next load starts. */
    fun onPageLoaded() {
        val progress = frame?.pageProgress ?: return
        if (progress.value !is DotNsLoadProgress.Failed) progress.value = DotNsLoadProgress.Completed
    }

    fun onPageFailed(cause: Throwable) {
        frame?.pageProgress?.value = DotNsLoadProgress.Failed(cause)
    }

    /**
     * The files the user picked for the page's `<input type="file">`, as a KYC check asks for an ID; `null` when the
     * picker was dismissed or failed, which releases the input so it can open again.
     */
    suspend fun pickFiles(params: FileChooserParams): Array<Uri>? =
        runCatching { WebFileChooserExecutor(contextManager.requireActivity(), params, fileProvider).execute() }
            .flatMap { it }
            .logFailure("Funding page file chooser failed")
            .getOrNull()

    /** The page reached its return URL; [url] carries the result in its query. */
    fun onPageReturned(url: String) = finish(FundingFrameOutcome.Returned(url))

    fun pauseConnections() {
        frame?.pauseConnections()
    }

    fun resumeConnections() {
        frame?.resumeConnections()
    }

    override fun onCleared() {
        super.onCleared()
        frame?.isShown = false
    }

    private fun hide() = launchUnit { frame?.let { router.closeFundingProviderFrame(it.request.entryId) } }

    private fun finish(outcome: FundingFrameOutcome) = launchUnit {
        val frame = frame ?: return@launchUnit
        service.answerFrame(frame.request, outcome)
        frames.release(frame.request.id)
        router.closeFundingProviderFrame(frame.request.entryId)
    }

    private data class LoadState(val isContentVisible: Boolean, val failed: Boolean)
}
