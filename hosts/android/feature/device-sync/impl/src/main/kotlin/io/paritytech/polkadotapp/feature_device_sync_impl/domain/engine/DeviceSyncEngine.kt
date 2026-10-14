package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.compareTo
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.feature_device_sync_impl.data.storage.SyncUpdateIdProvider
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.session.OwnDeviceSession
import io.paritytech.polkadotapp.feature_device_sync_impl.domain.signaling.SyncPeerChannelSignaling
import io.paritytech.polkadotapp.feature_sso_api.domain.OwnDevicesJournal
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannel
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerConnectionLogger
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaConfiguration
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.isTerminal
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import kotlin.time.Duration.Companion.seconds

/**
 * Drives one inter-own-device WebRTC connection: (re)establishes the data channel, handles
 * offerId-based restart recovery, and hands the open channel to [DeviceSyncRunner] for the sync
 * protocol. Lex-smaller `statementAccountId` initiates.
 */
class DeviceSyncEngine(
    private val ourStatementAccountId: AccountId,
    private val session: OwnDeviceSession,
    private val peerChannelFactory: PeerChannelFactory,
    private val ownDevicesJournal: OwnDevicesJournal,
    private val updateIdProvider: SyncUpdateIdProvider,
    private val collector: SyncEntityCollector,
    private val applier: SyncEntityApplier,
    private val localSyncTrigger: LocalSyncTrigger,
    private val scope: CoroutineScope,
) {
    private companion object {
        private val CONNECT_TIMEOUT = 30.seconds
        private val RECONNECT_BACKOFF = 5.seconds
    }

    private val peerStatementAccountId = session.peer.statementAccountId

    private val runner = DeviceSyncRunner(
        peerStatementAccountId = peerStatementAccountId,
        peerLogId = session.peer.id,
        ownDevicesJournal = ownDevicesJournal,
        updateIdProvider = updateIdProvider,
        collector = collector,
        applier = applier,
        localSyncTrigger = localSyncTrigger,
    )

    @Volatile
    private var started = false

    fun start() {
        if (started) {
            Timber.w("DeviceSyncEngine: start() called more than once for peer ${session.peer.id}")
            return
        }
        started = true
        scope.launch { connectionLoop() }
    }

    fun dispose() {
        scope.cancel()
    }

    private suspend fun connectionLoop() {
        val signaling = SyncPeerChannelSignaling(
            communicationSession = session.communicationSession,
            scope = scope,
            onOfferIdDetermined = { ownDevicesJournal.saveLastSyncOfferId(peerStatementAccountId, it) },
        )
        while (currentCoroutineContext().isActive) {
            try {
                connectOnce(signaling)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Timber.e(e, "DeviceSyncEngine: connection attempt to ${session.peer.id} failed")
            }
            if (currentCoroutineContext().isActive) {
                delay(RECONNECT_BACKOFF)
                Timber.i("DeviceSyncEngine: reconnecting to ${session.peer.id}")
            }
        }
    }

    private suspend fun connectOnce(signaling: SyncPeerChannelSignaling) = coroutineScope {
        val isInitiator = isInitiator(ourStatementAccountId, peerStatementAccountId)
        val sessionId = pairSessionId(ourStatementAccountId, peerStatementAccountId)

        Timber.i("DeviceSyncEngine: connecting peer=${session.peer.id} initiator=$isInitiator sessionId=$sessionId")

        sendReconnectIfResuming(signaling)

        val channel = peerChannelFactory.createSingleConnection(
            signaling = signaling,
            mediaConfiguration = MediaConfiguration.None,
            scope = scope.childScope(supervised = true),
            isInitiator = isInitiator,
            logger = PeerConnectionLogger(sessionId),
        )
        channel.startConnection()

        try {
            // Run the sync until the channel drops OR the peer asks us to reconnect — whichever first.
            raceUntilFirst(
                { runConnection(channel) },
                { onPeerReconnectRequest(signaling) },
            )
        } finally {
            channel.dispose()
        }
    }

    // If we are resuming a prior attempt, ask the peer to drop the connection it may still hold for us.
    private suspend fun sendReconnectIfResuming(signaling: SyncPeerChannelSignaling) {
        val lastOfferId = ownDevicesJournal.getLastSyncOfferId(peerStatementAccountId) ?: return
        Timber.i("DeviceSyncEngine: sending reconnected(offerId=$lastOfferId) to ${session.peer.id}")
        signaling.sendReconnected(lastOfferId)
    }

    // Wait for the channel to open (within timeout), then run the sync protocol until it drops.
    private suspend fun runConnection(channel: PeerChannel) {
        when (channel.awaitOpenOutcome()) {
            OpenOutcome.OPENED -> {
                Timber.i("DeviceSyncEngine: data channel up with ${session.peer.id}")
                runner.run(channel)
            }

            OpenOutcome.TERMINAL ->
                Timber.w("DeviceSyncEngine: ${session.peer.id} went terminal before opening — reconnecting")

            OpenOutcome.TIMED_OUT ->
                Timber.w("DeviceSyncEngine: ${session.peer.id} did not open within $CONNECT_TIMEOUT — reconnecting")
        }
    }

    private suspend fun onPeerReconnectRequest(signaling: SyncPeerChannelSignaling) {
        signaling.awaitReconnectRequest()
        Timber.i("DeviceSyncEngine: peer ${session.peer.id} asked to reconnect — rebuilding")
    }

    private suspend fun PeerChannel.awaitOpenOutcome(): OpenOutcome {
        return withTimeoutOrNull(CONNECT_TIMEOUT) {
            val channelOpenFlow = flowOf {
                dataTransport.awaitOpen()
                OpenOutcome.OPENED
            }

            val channelTerminal = connectionState
                .filter { it.isTerminal() }
                .map { OpenOutcome.TERMINAL }

            merge(channelOpenFlow, channelTerminal).first()
        } ?: OpenOutcome.TIMED_OUT
    }

    // Runs blocks concurrently, returns when the FIRST finishes, cancels the rest.
    private suspend fun raceUntilFirst(vararg blocks: suspend () -> Unit) = coroutineScope {
        val racers = blocks.map { block -> async { block() } }
        select {
            racers.forEach { racer ->
                racer.onAwait { }
            }
        }
        racers.forEach { it.cancel() }
    }
}

/** Smaller-id-unsigned initiates. Same rule as in-game peer handshake. */
private fun isInitiator(ours: AccountId, peer: AccountId): Boolean {
    return ours.value.compareTo(peer.value, unsigned = true) < 0
}

/** Pair-stable id so both ends agree on the WebRTC session identifier. */
private fun pairSessionId(a: AccountId, b: AccountId): String {
    return if (isInitiator(a, b)) {
        "device-sync:${a.value.toHexString()}:${b.value.toHexString()}"
    } else {
        "device-sync:${b.value.toHexString()}:${a.value.toHexString()}"
    }
}

private enum class OpenOutcome { OPENED, TERMINAL, TIMED_OUT }
