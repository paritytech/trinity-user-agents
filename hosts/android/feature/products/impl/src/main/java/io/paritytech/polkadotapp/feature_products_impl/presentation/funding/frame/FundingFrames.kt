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
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.createFundingPageWebView
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.fundingPageOrigin
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.CoroutineContext

/** An external https page shown in the frame, and where it finishes. */
data class FundingExternalPage(
    val url: String,
    val returnUrl: String?,
)

/**
 * A provider's screen the core presents, and what it shows, for as long as the request stands rather than as long as
 * a fragment shows it. Leaving the frame, reopening it from the CASH card, or a configuration change keeps the
 * provider's App session or the external page where the user had got to, as in the middle of a KYC check or a card's
 * 3-D Secure step.
 */
class FundingFrame(
    val request: FundingFrameRequest,
    private val spaHost: SpaHost,
    private val appContext: Context,
    private val notifier: AppNotifier,
    dispatcher: CoroutineDispatcher,
) : ComputationalScope, MessageDisplay {
    override val coroutineContext: CoroutineContext = SupervisorJob() + dispatcher

    /** The external page; `null` for a route of the provider's App. */
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

    /** The origin of the page the external WebView is on, which the frame's chrome names. */
    val pageOrigin = MutableStateFlow(page?.let { originOf(it.url) })

    val pageProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)

    /** Whether a frame screen shows it now, so reopening the session does not open it twice. */
    @Volatile
    var isShown: Boolean = false

    /** The App's WebView, owned by its session; `null` for an external page. */
    val appWebView: StateFlow<WebView?>
        get() = session?.webView ?: MutableStateFlow(null)

    val loadProgress: StateFlow<DotNsLoadProgress>
        get() = session?.loadProgress ?: pageProgress

    /** The external page's WebView, made once on the application's context and moved onto each screen that shows it. */
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

    /** Tears the screen's content down once the request is answered or closed. */
    fun release() {
        cancel()
        pageView?.destroy()
        pageView = null
    }
}

/** "ramp.example" or "ramp.example:8443": the core's origin of [url], with its port only when not the default. */
fun originOf(url: String): String? =
    fundingPageOrigin(url)?.let { origin -> origin.host + origin.port?.let { ":$it" }.orEmpty() }

/** The provider screens the core presents and has not had answered, by request. */
@Singleton
class FundingFrames @Inject constructor(
    private val spaHost: SpaHost,
    @param:ApplicationContext private val appContext: Context,
    private val notifier: AppNotifier,
    private val dispatchers: CoroutineDispatchers,
) {
    private val frames = ConcurrentHashMap<String, FundingFrame>()

    fun add(request: FundingFrameRequest): FundingFrame =
        FundingFrame(request, spaHost, appContext, notifier, dispatchers.main).also { frames[request.id] = it }

    fun get(requestId: String): FundingFrame? = frames[requestId]

    /** The newest frame still waiting for session [entryId]. */
    fun waiting(entryId: String): FundingFrame? = frames.values.lastOrNull { it.request.entryId == entryId }

    fun release(requestId: String) {
        frames.remove(requestId)?.release()
    }

    fun releaseAll(entryId: String) {
        frames.values.filter { it.request.entryId == entryId }.forEach { release(it.request.id) }
    }
}
