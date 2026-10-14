package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import kotlinx.coroutines.CoroutineScope

class ChatWebViewProvider @AssistedInject constructor(
    @param:ApplicationContext private val context: Context,
    productWebChromeClientFactory: ProductWebChromeClient.Factory,
    webViewPermissionClientFactory: WebViewPermissionClientFactory,
    dispatchers: CoroutineDispatchers,
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val servingHostResolver: DotNsServingHostResolver,
    @Assisted private val config: ChatWebViewConfig,
    @Assisted private val scope: CoroutineScope,
) : WebViewProvider(dispatchers) {
    @AssistedFactory
    interface Factory {
        fun create(config: ChatWebViewConfig, scope: CoroutineScope): ChatWebViewProvider
    }

    private val productId: ProductId = config.productId
    private val workerScript: WorkerScript = config.workerScript

    override val callingProductIdProvider = FixedProductId(productId)

    private val permissionClient = webViewPermissionClientFactory.create(callingProductIdProvider, workerScript.baseUrl)

    // Serves the worker's archive by host, so the entry module's relative imports resolve too.
    private val dotNsContentClient = DotNsWebViewClient(
        dotNsResolver,
        dotNsTldProvider,
        servingHostResolver,
        frameEmbeddingResponseHeaders(allowIframes = false),
    )
    private val chromeClient = productWebChromeClientFactory.create(
        logPrefix = "Script $productId",
        callingProductIdProvider = callingProductIdProvider,
        scope = scope,
        onTitleReceived = null,
    )

    @SuppressLint("SetJavaScriptEnabled")
    override suspend fun createWebView(): WebView {
        return WebView(context).apply {
            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                allowFileAccess = false
                allowContentAccess = false
            }
            webViewClient = object : WebViewClient() {
                override fun onPageFinished(view: WebView?, url: String?) {
                    notifyOnPageFinished()
                }

                override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                    notifyOnPageStarted(url)
                }

                override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
                    discardDeadWebView(view)
                    return true
                }

                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest?): WebResourceResponse? {
                    return permissionClient.shouldInterceptRequest(view, request)
                        ?: request?.let { dotNsContentClient.shouldInterceptRequest(view, it) }
                }
            }
            webChromeClient = chromeClient
        }
    }

    fun useTrUAPIPermissions(execution: TrUAPIProductExecution) {
        permissionClient.useTrUAPIPermissions(productId, execution, scope)
    }

    override suspend fun loadInitialContent() {
        accessWebView {
            it.loadDataWithBaseURL(
                getBaseUrl(),
                """
                    <!DOCTYPE html>
                    <html>
                    <head></head>
                    <body></body>
                    </html>
                """.trimIndent(),
                "text/html",
                "UTF-8",
                null,
            )
        }
    }

    private fun getBaseUrl() = workerScript.baseUrl
}

data class ChatWebViewConfig(
    val productId: ProductId,
    val workerScript: WorkerScript,
)
