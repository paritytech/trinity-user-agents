package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncEntityScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncMessageScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncUpdateAckScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncUpdateScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.storage.SyncUpdateIdProvider
import io.paritytech.polkadotapp.feature_sso_api.domain.OwnDevicesJournal
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.isTerminal
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

/**
 * Runs the device-sync data protocol over an already-open [PeerChannel] until it goes terminal:
 * push entities accumulated since the peer's checkpoint, apply inbound updates, advance the
 * checkpoint on ack. The connection lifecycle (open / reconnect) is owned by [DeviceSyncEngine].
 */
class DeviceSyncRunner(
    private val peerStatementAccountId: AccountId,
    private val peerLogId: String,
    private val ownDevicesJournal: OwnDevicesJournal,
    private val updateIdProvider: SyncUpdateIdProvider,
    private val collector: SyncEntityCollector,
    private val applier: SyncEntityApplier,
    private val localSyncTrigger: LocalSyncTrigger,
) {
    private companion object {
        private const val SYNC_USE_CASE = "device-sync"
        private val ACK_TIMEOUT = 30.seconds
    }

    private class InFlight(val id: UInt, val ack: CompletableDeferred<Unit>)

    private val retrySignal = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    @Volatile
    private var inFlight: InFlight? = null

    /** Runs until [channel] goes terminal, then returns so the engine can rebuild. */
    suspend fun run(channel: PeerChannel) = coroutineScope {
        try {
            val syncJob = launch {
                channel.dataTransport.subscribeMessages(SYNC_USE_CASE)
                    .onEach { channel.handleIncoming(it) }
                    .launchIn(this)
                channel.runActiveSync()
            }
            channel.connectionState.first { it.isTerminal() }
            Timber.w("DeviceSyncRunner: connection to $peerLogId went terminal — reconnecting")
            syncJob.cancelAndJoin()
        } finally {
            inFlight = null
        }
    }

    private suspend fun PeerChannel.runActiveSync() {
        merge(localSyncTrigger.observe(), retrySignal)
            .conflate()
            .collect { runSyncRound() }
    }

    // Checkpoint advances only on ack; on timeout it stays so the next change retries (receiver dedups).
    private suspend fun PeerChannel.runSyncRound() {
        val checkpoint = ownDevicesJournal.getOutgoingUpdateTime(peerStatementAccountId) ?: 0L
        // Captured after the read so the new checkpoint covers exactly what we just collected.
        val timePoint = System.currentTimeMillis()
        val entities = collector.collect(Instant.fromEpochMilliseconds(checkpoint))

        if (entities.isEmpty()) {
            Timber.i("DeviceSyncRunner: nothing to push to $peerLogId (checkpoint=$checkpoint)")
            // Advance anyway so we don't re-scan the empty window next time.
            ownDevicesJournal.updateOutgoingUpdateTime(peerStatementAccountId, timePoint)
            return
        }

        val id = updateIdProvider.nextId()
        val ack = CompletableDeferred<Unit>()
        inFlight = InFlight(id, ack)

        val update = SyncUpdateScale(id = id, entities = entities, timePoint = timePoint.toULong())
        val payload = BinaryScale.encodeToByteArray<SyncMessageScale>(SyncMessageScale.Update(update))
        dataTransport.send(SYNC_USE_CASE, payload)
        Timber.i("DeviceSyncRunner: pushed update id=$id to $peerLogId — payload=${payload.size} bytes, ${entities.summary()}")

        val acked = withTimeoutOrNull(ACK_TIMEOUT) { ack.await() } != null
        inFlight = null

        if (acked) {
            ownDevicesJournal.updateOutgoingUpdateTime(peerStatementAccountId, timePoint)
            Timber.i("DeviceSyncRunner: acked update id=$id, advanced checkpoint for $peerLogId to $timePoint")
        } else {
            Timber.w("DeviceSyncRunner: no ack for update id=$id within $ACK_TIMEOUT — re-arming retry")
            retrySignal.emit(Unit)
        }
    }

    private suspend fun PeerChannel.handleIncoming(payload: ByteArray) {
        val message = BinaryScale.decodeFromByteArrayCatching<SyncMessageScale>(payload)
            .onFailure { Timber.w(it, "DeviceSyncRunner: failed to decode incoming sync message") }
            .getOrNull() ?: return

        when (message) {
            is SyncMessageScale.Update -> {
                Timber.i("DeviceSyncRunner: applying update id=${message.update.id} from $peerLogId — ${message.update.entities.summary()}")
                applier.apply(message.update)
                send(SyncMessageScale.Ack(SyncUpdateAckScale(message.update.id)))
                Timber.i("DeviceSyncRunner: applied update id=${message.update.id} from $peerLogId, sent ack")
            }

            is SyncMessageScale.Ack -> applyAck(message.ack)
        }
    }

    private fun applyAck(ack: SyncUpdateAckScale) {
        val current = inFlight
        if (current != null && current.id == ack.id) {
            current.ack.complete(Unit)
        } else {
            Timber.w("DeviceSyncRunner: received ack for unknown/stale update id=${ack.id} from $peerLogId")
        }
    }

    private suspend fun PeerChannel.send(message: SyncMessageScale) {
        val bytes = BinaryScale.encodeToByteArray<SyncMessageScale>(message)
        dataTransport.send(SYNC_USE_CASE, bytes)
    }
}

internal fun List<SyncEntityScale>.summary(): String {
    val devices = filterIsInstance<SyncEntityScale.Devices>().sumOf { it.devices.size }
    val added = filterIsInstance<SyncEntityScale.ChatsAdded>().sumOf { it.chats.size }
    val removed = filterIsInstance<SyncEntityScale.ChatsRemoved>().sumOf { it.chats.size }
    val messages = filterIsInstance<SyncEntityScale.Messages>().sumOf { it.messages.size }
    return "Devices=$devices, ChatsAdded=$added, ChatsRemoved=$removed, Messages=$messages"
}
