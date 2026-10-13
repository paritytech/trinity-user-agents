package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.webkit.WebView
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContext
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
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

/**
 * A provider's own screen, for as long as the provider needs it. The session is already running, so back out
 * of it only hides it and leaves the provider waiting, which the CASH card can show again. Saying a bank
 * transfer was sent answers `Closed`; the overlay answers it too once the payment is seen or the session is over.
 */
@HiltViewModel
class FundingProviderFrameViewModel @Inject constructor(
    private val context: FundingFrameContext,
    private val router: ProductsRouter,
    spaHost: SpaHost,
) : BaseViewModel() {
    private val session = spaHost.createSession(context.url)

    val webView: StateFlow<WebView?> = session.webView

    val state: StateFlow<FundingProviderFrameUiState> = session.loadProgress
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

    fun onSentFunds() = finish(ProviderFrameOutcome.CLOSED)

    fun pauseConnections() = session.pauseConnections()

    fun resumeConnections() = session.resumeConnections()

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
}
