package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.MutableContextWrapper
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.net.Uri
import android.view.ViewGroup
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingExternalPage
import kotlinx.coroutines.launch
import uniffi.truapi.fundingPageAllowsMedia
import uniffi.truapi.fundingPageAllowsNavigation
import uniffi.truapi.fundingPageMatchesReturn

/** Back navigation inside the external page, which the frame's own back button tries first. */
class FundingExternalPageState {
    internal var webView: WebView? = null

    /** Loads the current page again, as after a failed load. */
    fun reload() {
        webView?.reload()
    }

    /** Goes back a page and answers `true`, or answers `false` on the first page. */
    fun goBack(): Boolean {
        val view = webView ?: return false
        if (!view.canGoBack()) return false
        view.goBack()
        return true
    }
}

@Composable
fun rememberFundingExternalPageState(): FundingExternalPageState = remember { FundingExternalPageState() }

/** A WebView for an onramp's own https page at [url], with what KYC and card checks need: scripts, forms and storage. */
@SuppressLint("SetJavaScriptEnabled")
fun createFundingPageWebView(context: Context, url: String): WebView = WebView(context).apply {
    settings.apply {
        javaScriptEnabled = true
        domStorageEnabled = true
        allowFileAccess = false
        allowContentAccess = false
        mediaPlaybackRequiresUserGesture = false
        // A popup the page opens loads in place, so the frame never leaves its own chrome.
        javaScriptCanOpenWindowsAutomatically = true
        setSupportMultipleWindows(false)
    }
    loadUrl(url)
}

/**
 * Shows [webView], an onramp's page that outlives this screen, moved onto this screen's context while it shows it:
 * popups open in place, the camera and file uploads through [onPickFiles]. Reaching [FundingExternalPage.returnUrl]
 * finishes it with the URL the page landed on.
 */
@Composable
fun FundingExternalPageView(
    webView: WebView,
    page: FundingExternalPage,
    state: FundingExternalPageState,
    onStarted: (String) -> Unit,
    onNavigated: (String) -> Unit,
    onLoaded: () -> Unit,
    onFailed: (Throwable) -> Unit,
    onReturned: (String) -> Unit,
    onPickFiles: suspend (WebChromeClient.FileChooserParams) -> Array<Uri>?,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pickFiles by rememberUpdatedState(onPickFiles)
    val started by rememberUpdatedState(onStarted)
    val navigated by rememberUpdatedState(onNavigated)
    val loaded by rememberUpdatedState(onLoaded)
    val failed by rememberUpdatedState(onFailed)
    val returned by rememberUpdatedState(onReturned)

    var pendingCapture by remember { mutableStateOf<PermissionRequest?>(null) }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        pendingCapture?.let { request -> if (granted) request.grant(request.resources) else request.deny() }
        pendingCapture = null
    }

    DisposableEffect(webView) {
        webView.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val url = request.url.toString()
                if (request.isForMainFrame && page.returnUrl != null && fundingPageMatchesReturn(url, page.returnUrl)) {
                    returned(url)
                    return true
                }
                // The core says what may load: any other scheme would hand the user to an app outside the frame.
                return !fundingPageAllowsNavigation(url, request.isForMainFrame)
            }

            override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
                if (page.returnUrl != null && fundingPageMatchesReturn(url, page.returnUrl)) {
                    view.stopLoading()
                    returned(url)
                } else {
                    started(url)
                }
            }

            // A redirect or a history change lands the main frame on a URL its start did not name.
            override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
                navigated(url)
            }

            override fun onPageFinished(view: WebView, url: String) {
                loaded()
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                if (request.isForMainFrame) failed(IllegalStateException(error.description.toString()))
            }
        }

        webView.webChromeClient = object : WebChromeClient() {
            // The camera only, which ID checks need, and only for the on-screen page's own origin, as the core rules.
            override fun onPermissionRequest(request: PermissionRequest) {
                val pageUrl = webView.url
                val allowed = pageUrl != null && fundingPageAllowsMedia(pageUrl, request.origin.toString())
                if (!allowed || request.resources.any { it != PermissionRequest.RESOURCE_VIDEO_CAPTURE }) {
                    request.deny()
                    return
                }
                val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                    PackageManager.PERMISSION_GRANTED
                if (granted) {
                    request.grant(request.resources)
                } else {
                    pendingCapture?.deny()
                    pendingCapture = request
                    cameraPermission.launch(Manifest.permission.CAMERA)
                }
            }

            override fun onPermissionRequestCanceled(request: PermissionRequest) {
                if (pendingCapture === request) pendingCapture = null
            }

            // Without this the page's <input type="file"> does nothing, and an ID upload never starts.
            override fun onShowFileChooser(
                webView: WebView,
                filePathCallback: ValueCallback<Array<Uri>>,
                fileChooserParams: FileChooserParams,
            ): Boolean {
                // The callback is answered exactly once, even with null, or the input stays locked.
                scope.launch { filePathCallback.onReceiveValue(pickFiles(fileChooserParams)) }
                return true
            }
        }
        state.webView = webView

        onDispose {
            // The page outlives this screen; callbacks into it must not.
            webView.webViewClient = WebViewClient()
            webView.webChromeClient = null
            if (state.webView === webView) state.webView = null
        }
    }

    AndroidView(
        modifier = modifier,
        factory = { hostContext -> FrameLayout(hostContext) },
        update = { host ->
            if (host.getChildAt(0) !== webView) {
                (webView.context as? MutableContextWrapper)?.baseContext = host.context
                (webView.parent as? ViewGroup)?.removeView(webView)
                host.addView(webView)
            }
        },
        onRelease = { host ->
            host.removeView(webView)
            (webView.context as? MutableContextWrapper)?.let { it.baseContext = it.applicationContext }
        },
    )
}
