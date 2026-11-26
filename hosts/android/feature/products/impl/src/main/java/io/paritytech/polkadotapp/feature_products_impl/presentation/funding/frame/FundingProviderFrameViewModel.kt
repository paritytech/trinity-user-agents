package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.net.Uri
import android.webkit.WebChromeClient.FileChooserParams
import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderFrames
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
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
import io.paritytech.polkadotapp.common.R as RCommon

data class FundingProviderFrameUiState(
    val showsSentFunds: Boolean,
    val isContentVisible: Boolean,
    val failed: Boolean,
)

// What it shows lives with the [FundingFrame], so backing out keeps the provider's page where it was.
@HiltViewModel
class FundingProviderFrameViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val frames: FundingFrames,
    private val sessions: FundingSessions,
    private val providerFrames: FundingProviderFrames,
    private val router: ProductsRouter,
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider,
) : BaseViewModel() {
    private val frame: FundingFrame? = frames.get(FundingFrameRequestId(savedStateHandle.get<String>(FundingProviderFrameBottomSheet.REQUEST_ID).orEmpty()))

    // No frame waits under this screen after process death, so it closes at once.
    val isOrphaned: Boolean = frame == null

    val page: FundingExternalPage? = frame?.page

    // Answered, or closed by the provider, while another screen sat over it.
    val isAnswered: Boolean
        get() = frame == null || frames.get(frame.request.id) == null

    val pageView: WebView? = frame?.pageView()

    val origin: StateFlow<String?> = frame?.pageOrigin ?: MutableStateFlow(null)

    val webView: StateFlow<WebView?> = frame?.appWebView ?: MutableStateFlow(null)

    private val awaitsUserTransfer = frame?.let { frame ->
        sessions.entryState(frame.request.intentId).map { it.progress?.awaitsUserTransfer == true }.onStart { emit(false) }
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

    fun onHide() = hide()

    fun onSentFunds() = finish(FundingFrameOutcome.Closed)

    fun onPageStarted(url: String) {
        frame?.pageOrigin?.value = originOf(url)
        frame?.pageProgress?.value = DotNsLoadProgress.Idle
    }

    fun onNavigated(url: String) {
        frame?.pageOrigin?.value = originOf(url)
    }

    // WebView finishes a failed load too; the failure stays until the next load starts.
    fun onPageLoaded() {
        val progress = frame?.pageProgress ?: return
        if (progress.value !is DotNsLoadProgress.Failed) progress.value = DotNsLoadProgress.Completed
    }

    fun onPageFailed(cause: Throwable) {
        frame?.pageProgress?.value = DotNsLoadProgress.Failed(cause)
    }

    // `null` when the picker was dismissed or failed, which releases the input so it can open again.
    suspend fun pickFiles(params: FileChooserParams): Array<Uri>? =
        runCancellableCatching { WebFileChooserExecutor(contextManager.requireActivity(), params, fileProvider).execute() }
            .flatMap { it }
            .getOrElse { failure ->
                showPresentationError(FundingFilesPresentationError(failure))
                null
            }

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

    private fun hide() = launchUnit { frame?.let { router.closeFundingProviderFrame(it.request.intentId) } }

    private fun finish(outcome: FundingFrameOutcome) = launchUnit {
        val frame = frame ?: return@launchUnit
        providerFrames.answer(frame.request, outcome)
        frames.release(frame.request.id)
        router.closeFundingProviderFrame(frame.request.intentId)
    }

    private data class LoadState(val isContentVisible: Boolean, val failed: Boolean)
}

private class FundingFilesPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.funding_frame_files_failed)
