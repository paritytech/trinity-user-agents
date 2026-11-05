@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import com.google.gson.Gson
import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.spyk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.request.DeliveryType
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.novasama.substrate_sdk_android.wsrpc.response.RpcResponse
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.encryption.HopTicketKeyDerivation
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSignature
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import javax.inject.Provider
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

val EXPECTED_HOP_REQUEST_BOUND = 60.seconds
const val HOP_SUBMIT_METHOD = "hop_submit"

private const val PUBLIC_KEY_SIZE = 32
private const val SIGNATURE_SIZE = 64
private const val EVENT_START = "start:"

fun startEvent(nodeUrl: String) = "$EVENT_START$nodeUrl"

class SentRequest(
    val socket: SocketService,
    val request: RuntimeRequest,
    val sentAt: Duration
)

class RecordingHopNodes(private val hopSigner: HopSigner) {
    private val signingKeypair: Sr25519Keypair = mockk()
    private val ticketKeyDerivation: HopTicketKeyDerivation = spyk(HopTicketKeyDerivation())

    val sockets = mutableListOf<SocketService>()
    val sentRequests = mutableListOf<SentRequest>()
    val events = mutableListOf<String>()

    fun hopService(scope: TestScope) = HopService(
        socketServiceProvider = Provider { newRecordingSocket(scope) },
        hopSigner = hopSigner,
        ticketKeyDerivation = ticketKeyDerivation
    )

    suspend fun withRealHopSigning() {
        every { signingKeypair.publicKey } returns ByteArray(PUBLIC_KEY_SIZE)
        every { ticketKeyDerivation.deriveSigningKeyPair(any()) } returns signingKeypair
        coEvery { hopSigner.sign(any()) } returns HopMultiSignature.SR25519(ByteArray(SIGNATURE_SIZE))
        coEvery { hopSigner.multiSigner() } returns HopMultiSigner.SR25519(ByteArray(PUBLIC_KEY_SIZE))
    }

    fun assertPendingSubmitsOnSeparateSessions(expected: Int) {
        assertEquals(expected, sentRequests.size)
        assertTrue(sentRequests.all { it.request.method == HOP_SUBMIT_METHOD })
        assertEquals(expected, sentRequests.map { it.socket }.distinct().size)
    }

    private fun newRecordingSocket(scope: TestScope): SocketService {
        val socket = mockk<SocketService>()
        every { socket.jsonMapper } returns Gson()
        every { socket.start(any(), any()) } answers { events += startEvent(firstArg()) }
        every { socket.stop() } just Runs
        every {
            socket.executeRequest(
                any<RuntimeRequest>(),
                any<DeliveryType>(),
                any<SocketService.ResponseListener<RpcResponse>>()
            )
        } answers {
            sentRequests += SentRequest(
                socket = socket,
                request = firstArg(),
                sentAt = scope.currentTime.milliseconds
            )
            mockk<SocketService.Cancellable> { every { cancel() } just Runs }
        }
        sockets += socket
        return socket
    }
}

fun TestScope.launchUpload(uploader: HopFileUploader, upload: FileUpload): Deferred<Result<Unit>> {
    val outcome = CompletableDeferred<Result<Unit>>()
    backgroundScope.launch { outcome.complete(runCatching { uploader.upload(upload) }) }
    return outcome
}

fun TestScope.advanceTo(virtualTime: Duration) {
    advanceTimeBy(virtualTime - currentTime.milliseconds)
    runCurrent()
}
