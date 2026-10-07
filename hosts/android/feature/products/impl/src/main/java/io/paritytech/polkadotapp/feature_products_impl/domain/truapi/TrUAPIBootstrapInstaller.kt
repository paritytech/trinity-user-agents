package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.ContainerScriptBundle
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

        val container = MEDIA_ISOLATION + "\n" +
            "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: true};\n" +
            ContainerScriptBundle.load(context)
        return { bootstrap ->
            { webView ->
                WebViewCompat.addDocumentStartJavaScript(webView, "if (window === window.top) {\n$bootstrap\n}", origins)
                WebViewCompat.addDocumentStartJavaScript(webView, container, setOf("*"))
            }
        }
    }

    private companion object {
        // Every frame, before the shared container. Product realms have no raw RTC,
        // capture or fullscreen escape even when the shared policy marker is absent.
        val MEDIA_ISOLATION = """
            (() => {
              const deny = (object, name) => { if (!object) return; try { Object.defineProperty(object, name, { value: undefined, writable: false, configurable: false }); } catch (_) {} };
              ['RTCPeerConnection','webkitRTCPeerConnection','mozRTCPeerConnection','RTCDataChannel','MediaStreamTrackProcessor','MediaStreamTrackGenerator'].forEach(name => deny(globalThis, name));
              ['getUserMedia','webkitGetUserMedia','mozGetUserMedia'].forEach(name => { deny(navigator, name); deny(Object.getPrototypeOf(navigator), name); });
              if (navigator.mediaDevices) ['getUserMedia','getDisplayMedia','enumerateDevices'].forEach(name => { deny(navigator.mediaDevices, name); deny(Object.getPrototypeOf(navigator.mediaDevices), name); });
              ['requestFullscreen','webkitRequestFullscreen','webkitRequestFullScreen'].forEach(name => deny(Element.prototype, name));
              ['webkitEnterFullscreen','webkitEnterFullScreen'].forEach(name => deny(HTMLVideoElement.prototype, name));
            })();
        """.trimIndent()
    }
}
