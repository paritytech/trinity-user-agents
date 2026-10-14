package io.parity.truapi

import android.net.Uri
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import okhttp3.OkHttpClient
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.AuthState
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.PermissionDecision
import uniffi.truapi.UserConfirmationReview
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import kotlin.io.path.createTempDirectory

/**
 * End-to-end host check: build the TrUAPI core with a local signing session
 * (no phone), start the localhost WS bridge, load the host playground in a
 * WebView through a fully functional [HostBridge] (real storage + chain
 * connectivity), and assert the product reaches a live TrUAPI `connected`
 * session — not just that a socket opened.
 *
 * `connected` is read from the playground's own e2e hook
 * (`window.__truapiPlaygroundE2E`, enabled by the `?e2e` query param), which
 * only reports connected after `System/handshake` and the account
 * connection-status wire round-trip through this host.
 *
 * Prereqs: run `yarn dev` in the truapi `playground/` and
 * `adb reverse tcp:3000 tcp:3000` so the device's `http://localhost:3000` maps
 * to the host dev server. Override the URL with an instrumentation arg:
 * `-Pandroid.testInstrumentationRunnerArguments.truapi.playgroundUrl=http://10.0.2.2:3000/`.
 */
@RunWith(AndroidJUnit4::class)
class TrUAPIDiagnosticsTest {

    @Test
    fun coreBoots_localSession_playgroundReachesConnected() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val args = InstrumentationRegistry.getArguments()

        // This test needs a live playground reachable from the device (yarn dev
        // + `adb reverse tcp:3000`). It is not self-contained, so skip it unless
        // explicitly opted in — otherwise a plain `connectedAndroidTest` run
        // would fail with no playground. Opt in with either
        // `-Pandroid.testInstrumentationRunnerArguments.truapi.runDiagnosis=1`
        // or by passing `truapi.playgroundUrl`.
        val optedIn = args.getString("truapi.runDiagnosis") == "1" ||
            args.getString("truapi.playgroundUrl") != null
        org.junit.Assume.assumeTrue(
            "skipped: set truapi.runDiagnosis=1 (needs a live playground on :3000)",
            optedIn,
        )

        val context = instrumentation.targetContext

        // The People-chain statement store backs the SSO/identity path; the
        // all-zero genesis is the local-session sentinel. Route any genesis to
        // the configured node so chain-touching methods can reach a backend.
        val chainNode = args.getString("truapi.chainWs")
        val chainProvider = WebSocketChainProvider({ listOfNotNull(chainNode) }, OkHttpClient())

        val authStates = ArrayList<AuthState>()
        val logs = ArrayList<String>()
        // Grants every prompt without UI — acceptable ONLY here, in a
        // non-interactive diagnostics run; confirmUserAction gates signing.
        val bridge = object : HostBridge {
            override val storage = PrefsHostStorage(
                context.getSharedPreferences("truapi_product_storage", android.content.Context.MODE_PRIVATE),
            )
            override val coreStorage = PrefsHostCoreStorage(
                context.getSharedPreferences("truapi_core_storage", android.content.Context.MODE_PRIVATE),
            )
            override fun onCoreLog(marker: String, detail: String) {
                synchronized(logs) { logs.add("$marker: $detail") }
            }
            override fun authStateChanged(state: AuthState) {
                synchronized(authStates) { authStates.add(state) }
            }
            override suspend fun navigateTo(url: String) = onCoreLog("truapi.host.navigate_to", url)
            override suspend fun devicePermission(
                product: ProductExecutionConfig,
                request: HostDevicePermissionRequest,
            ): PermissionDecision = PermissionDecision.ALLOW_ALWAYS
            override suspend fun remotePermission(
                product: ProductExecutionConfig,
                request: RemotePermission,
            ): PermissionDecision = PermissionDecision.ALLOW_ALWAYS
            override suspend fun confirmUserAction(review: UserConfirmationReview): Boolean = true
            override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean = false
            override fun chainConnect(genesisHash: ByteArray): UInt? = chainProvider.connect(genesisHash)
            override fun chainSend(connectionId: UInt, request: String) = chainProvider.send(connectionId, request)
            override fun chainClose(connectionId: UInt) = chainProvider.close(connectionId)
        }

        val config = HostRuntimeConfig(
            hostName = "Polkadot Android (diagnostics)",
            hostIcon = "https://dot.li/dotli.png",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi").toString(),
            // 32 bytes of BIP-39 entropy → a deterministic local signing session
            // (no SSO pairing, fully offline).
            localSessionSecret = ByteArray(32) { (it + 1).toByte() },
            localSessionLiteUsername = "android-diag",
        )

        val runtime = TrUAPIHostRuntime(bridge, config)
        val execution = runtime.openProductExecution(
            bridge = bridge,
            configuration = ProductExecutionConfig("dotli.dot", ProductExecutionKind.APP),
        )
        chainProvider.attach(
            onResponse = execution::notifyChainResponse,
            onClosed = execution::notifyChainClosed,
        )
        val endpoint = execution.startWsBridge()
        assertTrue("ws bridge port must be assigned", endpoint.port.toInt() > 0)
        assertTrue("ws bridge token must be non-empty", endpoint.token.isNotEmpty())

        // The local session emits a Connected auth state offline, before any
        // product connects.
        val sawConnected = synchronized(authStates) {
            authStates.any { it is AuthState.Connected }
        }
        assertTrue("expected a Connected auth state from the local session", sawConnected)

        val bootstrap = LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token)
        val container = ContainerScriptBundle.load(context)
        val url = args.getString("truapi.playgroundUrl") ?: "http://localhost:3000/"
        val origin = Uri.parse(url).let { "${it.scheme}://${it.encodedAuthority}" }

        // The playground is a static export; a `?e2e` query param does not
        // survive its client-side routing, so enable the e2e hook via the
        // localStorage fallback (`truapi:playground:e2e=1`) before the app mounts.
        val webViewRef = AtomicReference<WebView>()
        instrumentation.runOnMainSync {
            val wv = WebView(context)
            wv.settings.javaScriptEnabled = true
            wv.settings.domStorageEnabled = true
            wv.webViewClient = WebViewClient()
            check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT))
            WebViewCompat.addDocumentStartJavaScript(
                wv,
                "if (window === window.top) {\n$bootstrap\n" +
                    "window.localStorage.setItem('truapi:playground:e2e','1');\n}",
                setOf(origin),
            )
            WebViewCompat.addDocumentStartJavaScript(wv, container, setOf("*"))
            wv.loadUrl(url)
            webViewRef.set(wv)
        }

        // Poll the playground e2e hook until it reports a live `connected`
        // session (or time out). This proves the full host<->core<->product
        // wire — handshake + account connection status — works, not just that
        // the socket opened.
        val connected = pollForConnected(instrumentation, webViewRef, timeoutSec = 45)

        instrumentation.runOnMainSync { webViewRef.get().destroy() }
        // Teardown order matters: stop feeding the core from socket callbacks
        // (detach) and close sockets before closing the core, so no chain
        // callback races with execution.close().
        chainProvider.closeAll()
        chainProvider.detach()
        execution.stopWsBridge()
        execution.close()
        runtime.close()

        assertTrue(
            "product never reached connected; logs=${synchronized(logs) { logs.toList() }}",
            connected,
        )
    }

    /**
     * Drive the playground's `waitForConnectionStatus("connected")` — the
     * passive `connectionStatus()` getter never updates on its own; only
     * `waitForConnectionStatus` subscribes and advances the hook's status. We
     * kick it off once (storing its resolution on a window global) and poll
     * that global from Kotlin until it resolves `connected` or times out.
     */
    private fun pollForConnected(
        instrumentation: android.app.Instrumentation,
        webViewRef: AtomicReference<WebView>,
        timeoutSec: Long,
    ): Boolean {
        val timeoutMs = TimeUnit.SECONDS.toMillis(timeoutSec)
        val kickoff = """
            (function(){
              if (window.__truapiConnResult) return 'already';
              var hook = window.__truapiPlaygroundE2E;
              if (!hook || !hook.waitForConnectionStatus) return 'no-hook';
              hook.waitForConnectionStatus('connected', $timeoutMs)
                .then(function(){ window.__truapiConnResult = 'connected'; })
                .catch(function(e){ window.__truapiConnResult = 'error: ' + (e && e.message || e); });
              return 'started';
            })()
        """.trimIndent()
        // Retry the kickoff until the hook exists (it installs after the reload).
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(timeoutSec + 10)
        var started = false
        while (System.nanoTime() < deadline) {
            if (!started) {
                started = evalString(instrumentation, webViewRef, kickoff)
                    .let { it.contains("started") || it.contains("already") }
            }
            val result = evalString(
                instrumentation,
                webViewRef,
                "window.__truapiConnResult || 'pending'",
            )
            if (result.contains("connected")) return true
            if (result.contains("error:")) {
                android.util.Log.i("TRUAPI_PROBE", "waitForConnectionStatus $result")
                return false
            }
            Thread.sleep(500)
        }
        return false
    }

    /** Evaluate [js] on the main thread and return the (JSON-encoded) result string. */
    private fun evalString(
        instrumentation: android.app.Instrumentation,
        webViewRef: AtomicReference<WebView>,
        js: String,
    ): String {
        val result = AtomicReference<String>("")
        val done = java.util.concurrent.CountDownLatch(1)
        instrumentation.runOnMainSync {
            webViewRef.get().evaluateJavascript(js) { value ->
                result.set(value ?: "")
                done.countDown()
            }
        }
        done.await(5, TimeUnit.SECONDS)
        return result.get()
    }
}
