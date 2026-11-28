package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.content.Context
import android.content.MutableContextWrapper
import android.webkit.WebView
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.success
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.createFundingPageWebView
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.fundingPageOrigin
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.CoroutineContext

data class FundingExternalPage(
    val url: String,
    val returnUrl: String?,
)

// Lives as long as the request, not the fragment, so leaving, reopening or a configuration change keeps a KYC check
// or a 3-D Secure step where the user had got to.
class FundingFrame(
    val request: FundingFrameRequest,
    private val spaHost: SpaHost,
    private val appContext: Context,
    private val notifier: AppNotifier,
    dispatcher: CoroutineDispatcher,
) : ComputationalScope, MessageDisplay {
    override val coroutineContext: CoroutineContext = SupervisorJob() + dispatcher

    val page: FundingExternalPage? = (request.target as? FundingFrameTarget.Page)?.let {
        FundingExternalPage(url = it.url, returnUrl = it.returnUrl)
    }

    private val url: String = when (val target = request.target) {
        is FundingFrameTarget.Page -> target.url
        // The core checked the route, which is the path inside the App.
        is FundingFrameTarget.App -> request.providerId.toUrl().trimEnd('/') + target.route
    }

    private val session: SpaHostSession? by lazy { if (page == null) spaHost.createSession(url) else null }
    private var pageView: WebView? = null

    val pageOrigin = MutableStateFlow(page?.let { originOf(it.url) })

    val pageProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)

    // Reopening the session must not open it twice.
    @Volatile
    var isShown: Boolean = false

    val appWebView: StateFlow<WebView?>
        get() = session?.webView ?: MutableStateFlow(null)

    val loadProgress: StateFlow<DotNsLoadProgress>
        get() = session?.loadProgress ?: pageProgress

    // Made once on the application's context and moved onto each screen that shows it.
    fun pageView(): WebView? = page?.let { page ->
        pageView ?: createFundingPageWebView(MutableContextWrapper(appContext), page.url).also { pageView = it }
    }

    fun pauseConnections() {
        session?.pauseConnections()
    }

    fun resumeConnections() {
        session?.resumeConnections()
    }

    override fun showMessage(text: String) = notifier.success(text)

    fun release() {
        cancel()
        pageView?.destroy()
        pageView = null
    }
}

// "ramp.example" or "ramp.example:8443": the port only when not the default.
fun originOf(url: String): String? =
    fundingPageOrigin(url)?.let { origin -> origin.host + origin.port?.let { ":$it" }.orEmpty() }

@Singleton
class FundingFrames @Inject constructor(
    private val spaHost: SpaHost,
    @param:ApplicationContext private val appContext: Context,
    private val notifier: AppNotifier,
    private val dispatchers: CoroutineDispatchers,
) {
    // In the order they were added, so the last one for a session is its newest.
    private val frames = LinkedHashMap<FundingFrameRequestId, FundingFrame>()

    fun add(request: FundingFrameRequest): FundingFrame {
        val frame = FundingFrame(request, spaHost, appContext, notifier, dispatchers.main)
        synchronized(frames) { frames[request.id] = frame }
        return frame
    }

    fun get(requestId: FundingFrameRequestId): FundingFrame? = synchronized(frames) { frames[requestId] }

    fun waiting(intentId: FundingIntentId): FundingFrame? =
        synchronized(frames) { frames.values.lastOrNull { it.request.intentId == intentId } }

    fun release(requestId: FundingFrameRequestId) {
        synchronized(frames) { frames.remove(requestId) }?.release()
    }

    fun releaseAll(intentId: FundingIntentId) {
        val released = synchronized(frames) {
            frames.values.filter { it.request.intentId == intentId }.onEach { frames.remove(it.request.id) }
        }
        released.forEach(FundingFrame::release)
    }
}
