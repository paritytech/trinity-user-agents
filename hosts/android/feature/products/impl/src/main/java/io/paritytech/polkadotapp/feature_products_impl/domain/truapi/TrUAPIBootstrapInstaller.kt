package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.Build
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.ContainerScriptBundle
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import uniffi.truapi.truapiHostVersion
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
    fun installerFor(origins: Set<String>, hostSessionId: String?): (bootstrap: String) -> (WebView) -> Unit {
        val telemetryScript = if (hostSessionId != null && FeatureOption.HOST_TELEMETRY.isEnabled) {
            runCatching { hostTelemetryScript(context, hostSessionId) }
                .logFailure("Native host telemetry unavailable")
                .getOrNull()
        } else {
            null
        }
        check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            "WebView lacks DOCUMENT_START_SCRIPT; cannot run a TrUAPI product"
        }

        val container = MEDIA_ISOLATION + "\n" +
            "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: true};\n" +
            ContainerScriptBundle.load(context)
        return { bootstrap ->
            { webView ->
                val mainFrameBootstrap = listOfNotNull(telemetryScript, bootstrap).joinToString("\n")
                val pageBootstrap = "if (window === window.top) {\n$mainFrameBootstrap\n}"
                WebViewCompat.addDocumentStartJavaScript(webView, pageBootstrap, origins)
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

internal fun hostTelemetryScript(
    context: Context,
    hostSessionId: String,
): String {
    val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
    val telemetry = hostTelemetryJson.encodeToString(
        HostTelemetryBootstrap(
            hostSessionId = hostSessionId,
            appVersion = packageInfo.versionName?.takeIf(String::isNotBlank),
            appBuild = packageInfo.longVersionCode.toString(),
            appVariant = BuildConfig.BUILD_TYPE,
            platform = "android",
            osVersion = Build.VERSION.RELEASE?.takeIf(String::isNotBlank),
            truapiHostVersion = truapiHostVersion().takeIf(String::isNotBlank),
            schemaVersion = "1",
        ),
    )

    return """
        try {
          if (!Object.prototype.hasOwnProperty.call(globalThis, "__trinityHostTelemetry")) {
            Object.defineProperty(globalThis, "__trinityHostTelemetry", {
              value: Object.freeze($telemetry),
              writable: false,
              configurable: false
            });
          }
        } catch {}
    """.trimIndent()
}

private val hostTelemetryJson = Json { explicitNulls = false }

@Serializable
private data class HostTelemetryBootstrap(
    val hostSessionId: String,
    val appVersion: String?,
    val appBuild: String,
    val appVariant: String,
    val platform: String,
    val osVersion: String?,
    val truapiHostVersion: String?,
    val schemaVersion: String,
)
