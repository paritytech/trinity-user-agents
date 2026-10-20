package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.Build
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.ContainerScriptBundle
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import timber.log.Timber
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
    fun installerFor(origins: Set<String>, hostSessionId: String? = null): (bootstrap: String) -> (WebView) -> Unit {
        val telemetryScript = if (hostSessionId != null && shouldInstallHostTelemetry(context.packageName)) {
            runCatching { hostTelemetryScript(context, hostSessionId) }
                .onFailure { error -> Timber.w(error, "Native host telemetry unavailable") }
                .getOrNull()
        } else {
            null
        }
        return installerForScript(origins, telemetryScript)
    }

    internal fun installerForScript(
        origins: Set<String>,
        telemetryScript: String?,
    ): (bootstrap: String) -> (WebView) -> Unit {
        check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            "WebView lacks DOCUMENT_START_SCRIPT; cannot run a TrUAPI product"
        }

        val container = "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: true};\n" +
            ContainerScriptBundle.load(context)
        return { bootstrap ->
            { webView ->
                val mainFrameBootstrap = listOfNotNull(telemetryScript, bootstrap).joinToString("\n")
                val pageBootstrap = "if (window === window.top) {\n$mainFrameBootstrap\n}"
                WebViewCompat.addDocumentStartJavaScript(webView, pageBootstrap, origins)
                WebViewCompat.addDocumentStartJavaScript(webView, container, setOf("*"))
                (webView.webChromeClient as? ProductWebChromeClient)?.useContainerPermissions()
            }
        }
    }
}

internal fun hostTelemetryScript(
    context: Context,
    hostSessionId: String,
    hostVersion: String = truapiHostVersion(),
    osVersion: String = Build.VERSION.RELEASE,
): String {
    val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
    val packageName = context.packageName
    val appVariant = when {
        packageName.endsWith(".nightly") -> "nightly"
        packageName.endsWith(".safetynet") -> "safetynet"
        packageName.endsWith(".debug") -> "debug"
        else -> "release"
    }
    val telemetry = buildJsonObject {
        put("hostSessionId", hostSessionId)
        packageInfo.versionName?.takeIf(String::isNotBlank)?.let { put("appVersion", it) }
        put("appBuild", packageInfo.longVersionCode.toString())
        put("appVariant", appVariant)
        put("platform", "android")
        osVersion.takeIf(String::isNotBlank)?.let { put("osVersion", it) }
        hostVersion.takeIf(String::isNotBlank)?.let { put("truapiHostVersion", it) }
        put("schemaVersion", "1")
    }

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

internal fun shouldInstallHostTelemetry(packageName: String): Boolean =
    listOf(".nightly", ".safetynet", ".debug").any(packageName::endsWith)
