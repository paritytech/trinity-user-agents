package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.tools_media_connection_api.domain.DataTransportState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseData
import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseId
import io.paritytech.polkadotapp.tools_media_connection_impl.models.DataChannelMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import org.webrtc.DataChannel
import java.nio.ByteBuffer
import kotlin.time.Duration.Companion.milliseconds

class DataChannelMessaging(
    scope: CoroutineScope,
    private val logger: PeerConnectionLogger
) : CoroutineScope by scope {
    private companion object {
        val SEND_BUFFER_LIMITS = SendBufferLimits(pauseAbove = 1.megabytes, resumeBelow = 256.kilobytes)
        val BUFFER_DRAIN_POLL = 100.milliseconds
    }

    private var dataChannel: DataChannel? = null

    // Unbounded: a dropped frame is a lost sync update or file chunk.
    private val messagesMutable = MutableSharedFlow<DataChannelMessage>(extraBufferCapacity = Channel.UNLIMITED)
    val messages = messagesMutable.asSharedFlow()

    private val stateMutable = MutableStateFlow(DataTransportState.Connecting)
    val state = stateMutable.asStateFlow()

    // Mutex is fair, so use cases sharing the channel interleave instead of one draining its backlog first.
    private val sendMutex = Mutex()
    private val bufferedAmountChanged = Channel<Unit>(Channel.CONFLATED)

    private val observer = createObserver()

    fun init(channel: DataChannel) {
        if (dataChannel != null) {
            logger.log("DataChannelMessaging: already initialized")
            return
        }
        logger.log("DataChannelMessaging: init with channel ${channel.label()}")
        dataChannel = channel
        channel.registerObserver(observer)
    }

    suspend fun send(message: DataChannelMessage) {
        logger.log("DataChannelMessaging: sending message id=${message.id}")
        awaitChannelOpen()

        val encoded = BinaryScale.encodeToByteArray(DataChannelMessage.serializer(), message)
        sendMutex.withLock {
            awaitSendCapacity(encoded.size.bytes)
            val buffer = DataChannel.Buffer(ByteBuffer.wrap(encoded), true)
            val success = dataChannel?.send(buffer)
            logger.log("DataChannelMessaging: message sent success=$success")
        }
    }

    fun subscribeMessages(id: UseCaseId): Flow<UseCaseData> {
        return messages
            .filter { it.id == id }
            .map { it.data }
    }

    suspend fun sendMessage(id: UseCaseId, data: UseCaseData) {
        send(DataChannelMessage(id = id, data = data))
    }

    // WebRTC closes the channel once its send buffer overflows, so wait for it to drain instead of queueing more.
    private suspend fun awaitSendCapacity(messageSize: InformationSize) {
        var throttled = false
        while (isOpen()) {
            val bufferedAmount = dataChannel?.bufferedAmount()?.bytes ?: return
            if (!SEND_BUFFER_LIMITS.shouldWait(bufferedAmount, messageSize, throttled)) return

            throttled = true
            withTimeoutOrNull(BUFFER_DRAIN_POLL) { bufferedAmountChanged.receive() }
        }
    }

    private fun createObserver() = object : DataChannel.Observer {
        override fun onBufferedAmountChange(previousAmount: Long) {
            bufferedAmountChanged.trySend(Unit)
        }

        override fun onStateChange() {
            dataChannel?.state()?.let { rawState ->
                stateMutable.value = rawState.toDataTransportState()
            }
        }

        override fun onMessage(buffer: DataChannel.Buffer) {
            val data = buffer.data
            val bytes = ByteArray(data.remaining())
            data.get(bytes)

            val message = runCatching {
                BinaryScale.decodeFromByteArray<DataChannelMessage>(bytes)
            }.getOrNull() ?: return

            logger.log("DataChannelMessaging: RX frame useCase=${message.id} ${bytes.size}b subscribers=${messagesMutable.subscriptionCount.value}")
            messagesMutable.tryEmit(message)
        }
    }

    suspend fun awaitChannelOpen() {
        logger.log("DataChannelMessaging: awaiting channel open")
        state.first { it == DataTransportState.Open }
        logger.log("DataChannelMessaging: channel is open")
    }

    fun isOpen(): Boolean = stateMutable.value == DataTransportState.Open

    fun dispose() {
        logger.log("DataChannelMessaging: dispose")
        stateMutable.value = DataTransportState.Closed
        dataChannel?.unregisterObserver()
        dataChannel?.close()
        dataChannel?.dispose()
        dataChannel = null
    }
}

private fun DataChannel.State.toDataTransportState(): DataTransportState = when (this) {
    DataChannel.State.CONNECTING -> DataTransportState.Connecting
    DataChannel.State.OPEN -> DataTransportState.Open
    DataChannel.State.CLOSING -> DataTransportState.Closing
    DataChannel.State.CLOSED -> DataTransportState.Closed
}
