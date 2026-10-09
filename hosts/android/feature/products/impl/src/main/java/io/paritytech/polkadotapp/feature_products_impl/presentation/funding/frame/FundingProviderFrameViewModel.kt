package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.webkit.WebView
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

data class FundingProviderFrameUiState(
    val showsSentFunds: Boolean,
    val isContentVisible: Boolean,
    val failed: Boolean,
)

/**
 * A provider's own screen, for as long as the provider needs it. Back out of it is `Dismissed`; it closes
 * itself as `Closed` once the payment is seen or the session is over, or, for a bank transfer, when the user
 * says they have sent the funds.
 */
@HiltViewModel
class FundingProviderFrameViewModel @Inject constructor(
    private val context: FundingFrameContext,
    private val frames: FundingFrameContexts,
    private val interactor: FundingFlowInteractor,
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
        launch {
            interactor.sessionChanges(context.intent)
                .onStart { emit(Unit) }
                .map { interactor.paymentMoved(context.intent) }
                .collect { moved -> if (moved) finish(ProviderFrameOutcome.CLOSED) }
        }
    }

    fun onBack() {
        val current = webView.value
        if (current != null && current.canGoBack()) current.goBack() else finish(ProviderFrameOutcome.DISMISSED)
    }

    fun onSentFunds() = finish(ProviderFrameOutcome.CLOSED)

    fun pauseConnections() = session.pauseConnections()

    fun resumeConnections() = session.resumeConnections()

    override fun onCleared() {
        super.onCleared()
        context.answer(ProviderFrameOutcome.DISMISSED)
        frames.remove(context.intent)
    }

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
