package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.ContainerScriptBundle
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import javax.inject.Inject

/**
 * Builds the WebView setup that runs a product's bootstrap script at document start, for the app
 * WebView and the hidden worker one alike.
 *
 * The support check belongs here rather than at either call site: taken after the execution is open,
 * a WebView without the feature throws from inside the attach callback and leaves a live loopback
 * listener that then blocks every re-attach.
 */
class TrUAPIBootstrapInstaller @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    fun installerFor(origins: Set<String>): (bootstrap: String) -> (WebView) -> Unit {
        check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            "WebView lacks DOCUMENT_START_SCRIPT; cannot run a TrUAPI product"
        }

        val container = "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: true};\n" +
            ContainerScriptBundle.load(context)
        return { bootstrap ->
            { webView ->
                WebViewCompat.addDocumentStartJavaScript(webView, "if (window === window.top) {\n$bootstrap\n}", origins)
                WebViewCompat.addDocumentStartJavaScript(webView, container, setOf("*"))
                (webView.webChromeClient as? ProductWebChromeClient)?.useContainerPermissions()
            }
        }
    }
}
