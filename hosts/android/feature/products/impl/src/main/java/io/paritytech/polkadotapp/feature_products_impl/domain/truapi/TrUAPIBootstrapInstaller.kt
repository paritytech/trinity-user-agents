package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.os.Build
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.ContainerScriptBundle
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject
import uniffi.truapi.truapiHostVersion

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
        val telemetryScript = if (hostSessionId != null && FeatureOption.HOST_TELEMETRY.isEnabled) {
            runCatching { hostTelemetryScript(context, hostSessionId) }
                .logFailure("Native host telemetry unavailable")
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
    buildType: String = BuildConfig.BUILD_TYPE,
): String {
    val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
    val telemetry = hostTelemetryJson.encodeToString(
        HostTelemetryBootstrap(
            hostSessionId = hostSessionId,
            appVersion = packageInfo.versionName?.takeIf(String::isNotBlank),
            appBuild = packageInfo.longVersionCode.toString(),
            appVariant = buildType,
            platform = "android",
            osVersion = osVersion.takeIf(String::isNotBlank),
            truapiHostVersion = hostVersion.takeIf(String::isNotBlank),
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
