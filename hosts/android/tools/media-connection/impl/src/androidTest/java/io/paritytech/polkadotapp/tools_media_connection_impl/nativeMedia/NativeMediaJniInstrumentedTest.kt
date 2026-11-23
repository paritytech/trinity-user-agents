package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.Manifest
import android.os.Build
import android.util.Base64
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import io.parity.truapi.HostStorage
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.tools_media_connection_impl.AudioCaptureState
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.ExternalRtcConfigProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnApi
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnCredentialsResponse
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnIssueRequestBody
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger
import kotlin.io.path.createTempDirectory
import kotlin.time.Clock
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.HostPushNotificationRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.NativeMediaBackendCommand
import uniffi.truapi.ProductContext
import uniffi.truapi.NativeMediaBackendCapabilities
import uniffi.truapi.NativeMediaBackendResponse
import uniffi.truapi.NativeMediaCallbacks
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind

@RunWith(AndroidJUnit4::class)
class NativeMediaJniInstrumentedTest {
    @Test
    fun capabilitiesCrossRustAsyncCallbackWithoutAnAccount(): Unit = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        if (Build.VERSION.SDK_INT >= 33) {
            instrumentation.uiAutomation.grantRuntimePermission(context.packageName, Manifest.permission.POST_NOTIFICATIONS)
        }
        val forbidden = ConcurrentLinkedQueue<String>()
        fun unavailable(name: String): Nothing {
            forbidden.add(name)
            error("Offline Media qualification unexpectedly requested $name")
        }
        val bridge = object : HostBridge {
            override val storage = object : HostStorage {
                private val values = ConcurrentHashMap<String, ByteArray>()
                override suspend fun read(key: String) = values[key]?.copyOf()
                override suspend fun write(key: String, value: ByteArray) { values[key] = value.copyOf() }
                override suspend fun clear(key: String) { values.remove(key) }
            }
            override val coreStorage = object : HostCoreStorage {
                override val storageIdentifier = "media-jni-regression-${UUID.randomUUID()}"
                private val values = ConcurrentHashMap<String, ByteArray>()
                private fun encodedKey(value: ByteArray) = Base64.encodeToString(value, Base64.NO_WRAP)
                override suspend fun read(key: ByteArray) = values[encodedKey(key)]?.copyOf()
                override suspend fun write(key: ByteArray, value: ByteArray) { values[encodedKey(key)] = value.copyOf() }
                override suspend fun clear(key: ByteArray) { values.remove(encodedKey(key)) }
            }
            override suspend fun navigateTo(url: String): Unit = unavailable("navigation")
            override suspend fun pushNotification(request: HostPushNotificationRequest): UInt = unavailable("push notification")
            override suspend fun devicePermission(product: ProductExecutionConfig, request: HostDevicePermissionRequest): uniffi.truapi.PermissionDecision = unavailable("device permission prompt")
            override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission): uniffi.truapi.PermissionDecision = unavailable("remote permission prompt")
            override suspend fun featureSupported(request: HostFeatureSupportedRequest): Boolean = unavailable("unrelated feature probe")
            override fun chainConnect(genesisHash: ByteArray): UInt? = unavailable("chain connection")
            override fun chainSend(connectionId: UInt, request: String): Unit = unavailable("chain request")
            override fun onCoreLog(marker: String, detail: String) {
                if (marker.endsWith("dispatch_error")) forbidden.add("core dispatch error: $detail")
            }
        }
        val config = ExternalRtcConfigProvider(
            object : TurnApi {
                override suspend fun issueTurnCredentials(body: TurnIssueRequestBody): TurnCredentialsResponse = unavailable("TURN credentials")
            },
            CurrentTimeContext { Clock.System.now() },
        )
        val core = WebRtcCore(context)
        val backend = withContext(Dispatchers.Main.immediate) {
            NativeMediaBackend(context, core, config, PRODUCT)
        }
        val capabilitiesCalls = AtomicInteger()
        val closeCalls = AtomicInteger()
        val media = object : NativeMediaCallbacks by backend {
            override suspend fun capabilities(product: ProductContext): NativeMediaBackendCapabilities {
                capabilitiesCalls.incrementAndGet()
                return backend.capabilities(product)
            }
            override suspend fun command(product: ProductContext, runtimeId: ULong, command: NativeMediaBackendCommand): NativeMediaBackendResponse {
                if (command != NativeMediaBackendCommand.CloseRuntime) unavailable("non-cleanup Media command")
                val response = backend.command(product, runtimeId, command)
                check(response == NativeMediaBackendResponse.Done)
                closeCalls.incrementAndGet()
                return response
            }
        }
        // No local signing secret, account, cached authorization, chain client, or TURN service.
        val runtime = TrUAPIHostRuntime(
            bridge,
            HostRuntimeConfig(
                hostName = "Offline Media JNI regression",
                peopleChainGenesisHash = ByteArray(32),
                bulletinChainGenesisHash = ByteArray(32),
                assetHubChainGenesisHash = ByteArray(32),
                networkSuffix = "paseo",
                databaseDirectory = createTempDirectory("truapi").toString(),
            ),
        )
        val client = OkHttpClient()
        try {
            withTimeout(45_000) {
                repeat(2) { index ->
                    val execution = runtime.openProductExecution(
                        bridge, ProductExecutionConfig(PRODUCT, ProductExecutionKind.APP), media = media,
                    )
                    try {
                        val endpoint = execution.startWsBridge()
                        val requestId = "media:$index"
                        val id = requestId.toByteArray(Charsets.UTF_8)
                        // frame.rs envelope + api/media.rs trait 218/get_capabilities 0 + V1 index 0.
                        val request = byteArrayOf((id.size shl 2).toByte()) + id + byteArrayOf(218.toByte(), 0, 0, 0)
                        val reply = CompletableDeferred<ByteArray>()
                        val socket = client.newWebSocket(
                            Request.Builder().url("ws://127.0.0.1:${endpoint.port}/?t=${endpoint.token}").build(),
                            object : WebSocketListener() {
                                override fun onOpen(webSocket: WebSocket, response: Response) {
                                    if (!webSocket.send(request.toByteString())) {
                                        reply.completeExceptionally(AssertionError("WebSocket refused the capability request"))
                                    }
                                }
                                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                                    reply.complete(bytes.toByteArray())
                                }
                                override fun onMessage(webSocket: WebSocket, text: String) {
                                    reply.completeExceptionally(AssertionError("Expected a binary SCALE response"))
                                }
                                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                                    // The bridge token is deliberately absent from assertion/log text.
                                    reply.completeExceptionally(AssertionError("Local JNI WebSocket failed: ${t.javaClass.simpleName}"))
                                }
                            },
                        )
                        try {
                            assertCapabilities(withTimeout(15_000) { reply.await() }, requestId)
                            assertTrue("Real NativeMediaBackend capabilities callback must have run", capabilitiesCalls.get() > index)
                            assertEquals(emptyList<String>(), forbidden.toList())
                            assertEquals(AudioCaptureState.IDLE, core.audioCaptureState.value)
                            Log.i(TAG, "Rust -> async Kotlin capabilities -> Rust -> SCALE reply succeeded for execution $index")
                        } finally {
                            socket.cancel()
                        }
                    } finally {
                        execution.close()
                    }
                    withTimeout(10_000) { while (closeCalls.get() <= index) delay(25) }
                }
                assertEquals(emptyList<String>(), forbidden.toList())
                Log.i(TAG, "native CloseRuntime returned Done twice; no authenticated identity, capture, or external services")
            }
        } finally {
            runtime.close()
            client.dispatcher.executorService.shutdown()
            client.connectionPool.evictAll()
            withContext(NonCancellable + Dispatchers.Main.immediate) {
                backend.close()
                core.peerConnectionFactory.dispose()
                core.eglBase.release()
            }
        }
    }

    private fun assertCapabilities(bytes: ByteArray, requestId: String) {
        val value = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        fun unsignedByte() = value.get().toInt() and 255
        fun unsignedShort() = value.short.toInt() and 65535
        fun compactSmall(): Int {
            val encoded = unsignedByte()
            check(encoded and 3 == 0) { "Unexpected SCALE collection length" }
            return encoded ushr 2
        }
        val id = ByteArray(compactSmall()).also { value.get(it) }
        assertEquals(requestId, id.toString(Charsets.UTF_8))
        assertEquals(20, unsignedByte())
        assertEquals(0, unsignedByte())
        assertEquals(1, unsignedByte())
        assertEquals("Capabilities must succeed, not Unsupported/HostFailure", 0, unsignedByte())
        assertEquals("V1 capability response", 0, unsignedByte())
        val versions = List(compactSmall()) { unsignedShort() }
        assertTrue(1 in versions)
        assertArrayEquals("Configured offline network must survive the JNI round trip", ByteArray(32), ByteArray(32).also { value.get(it) })
        val remoteLimit = unsignedShort()
        val sessionLimit = unsignedShort()
        val surfaceLimit = unsignedShort()
        assertTrue(remoteLimit >= 5)
        assertTrue(sessionLimit >= 1)
        assertTrue(surfaceLimit >= 2 * (remoteLimit + 1))
        repeat(4) { assertTrue("Runtime deadlines must be finite and positive", value.int > 0) }
        repeat(4) { assertTrue("Retention budgets must be positive", value.int > 0) }
        repeat(3) { assertTrue("Runtime queue/listener limits must be positive", unsignedShort() > 0) }
        assertEquals("Complete response must decode without trailing bytes", 0, value.remaining())
    }

    companion object {
        private const val TAG = "NativeMediaJniRegression"
        private const val PRODUCT = "media-regression.paseo"
    }
}
