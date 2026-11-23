package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import org.webrtc.*
import uniffi.truapi.NativeMediaRemoteState
import uniffi.truapi.NativeMediaTrackState
import uniffi.truapi.*
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

internal const val NATIVE_MEDIA_ICE_GATHERING_DEADLINE_MS = 1_000L
internal const val NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT = 256
private val nativeMediaIceFieldSeparator = Regex("\\s+")

/**
 * Confined to the peer's host dispatcher, like negotiation. Hold candidates until
 * the current local SDP is published, then send only candidates it did not carry.
 * Continual gathering has a per-description budget; optional excess is dropped.
 */
internal class NativeMediaLocalIce {
    private data class Key(val mline: Int, val fields: List<String>)
    private var active = true
    private var buffering = true
    private val pending = mutableListOf<IceCandidate>()
    private val advertised = mutableSetOf<Key>()
    private val ufrags = mutableListOf<String?>()

    fun begin() {
        active = true
        buffering = true
        pending.clear()
    }

    fun candidate(candidate: IceCandidate): Boolean {
        if (!active) return false
        if (buffering) {
            if (pending.size < NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT) pending.add(candidate)
            return false
        }
        if (advertised.size >= NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT) return false
        val fields = candidate.sdp.trim().split(nativeMediaIceFieldSeparator)
        val ufragIndex = fields.indexOf("ufrag")
        val ufrag = fields.getOrNull(ufragIndex + 1).takeIf { ufragIndex >= 0 }
        val expected = ufrags.getOrNull(candidate.sdpMLineIndex)
        // Delayed callbacks from an ICE generation replaced by a restart are stale.
        if (ufrag != null && expected != null && ufrag != expected) return false
        return advertised.add(key(candidate.sdpMLineIndex, fields))
    }

    fun publish(sdp: String): List<IceCandidate> {
        advertised.clear()
        ufrags.clear()
        var mline = -1
        var sessionUfrag: String? = null
        for (line in sdp.lineSequence()) {
            when {
                line.startsWith("m=") -> { mline++; ufrags.add(sessionUfrag) }
                line.startsWith("a=ice-ufrag:") -> {
                    val ufrag = line.substringAfter(':')
                    if (mline < 0) sessionUfrag = ufrag else ufrags[mline] = ufrag
                }
                mline >= 0 && line.startsWith("a=candidate:") &&
                    advertised.size < NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT ->
                    advertised.add(key(mline, line.substring(2).trim().split(nativeMediaIceFieldSeparator)))
            }
        }
        buffering = false
        val late = pending.filter(::candidate)
        pending.clear()
        return late
    }

    fun cancel() {
        active = false
        pending.clear()
        advertised.clear()
        ufrags.clear()
    }

    private fun key(mline: Int, fields: List<String>): Key {
        // WebRTC's SDP and callback serializers may differ in these extensions.
        // The ICE generation is checked separately against the SDP credentials.
        val identity = ArrayList<String>(fields.size)
        for (index in 0 until minOf(8, fields.size)) identity.add(fields[index])
        var index = 8
        while (index + 1 < fields.size) {
            val name = fields[index]
            if (name == "raddr" || name == "rport" || name == "tcptype") {
                identity.add(name)
                identity.add(fields[index + 1])
            }
            index += 2
        }
        return Key(mline, identity)
    }
}

/** No data channel and no application/Chat signaling. Every SDP/ICE message returns to Rust. */
internal class NativeMediaPeer(
    core: WebRtcCore,
    servers: List<PeerConnection.IceServer>,
    private val initialOfferer: Boolean,
    private val scope: CoroutineScope,
    private val emit: (NativeMediaBackendEvent) -> Unit,
    private val sessionId: ByteArray,
    private val participantId: ByteArray,
    private val picturesChanged: () -> Unit,
    private val released: () -> Unit,
) : AutoCloseable {
    private val negotiation = Mutex()
    private var closed = false
    private var makingOffer = false
    private var ignoreOffer = false
    private var ready = false
    private var dirty = false
    private val candidates = mutableListOf<IceCandidate>()
    private val remote = arrayOfNulls<MediaStreamTrack>(3)
    private val connection: PeerConnection
    private var slots = emptyList<RtpTransceiver>()
    private var localTracks: List<MediaStreamTrack?> = listOf(null, null, null)
    private var applyingRemoteDescription = false
    private val localIce = NativeMediaLocalIce()
    private var gatheringComplete: CompletableDeferred<Unit>? = null

    init {
        val observer = object : PeerConnection.Observer {
            override fun onSignalingChange(state: PeerConnection.SignalingState?) {}
            override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {}
            override fun onIceConnectionReceivingChange(receiving: Boolean) {}
            override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {
                background {
                    if (!closed && state == PeerConnection.IceGatheringState.COMPLETE &&
                        connection.iceGatheringState() == PeerConnection.IceGatheringState.COMPLETE) {
                        gatheringComplete?.complete(Unit)
                    }
                }
            }
            override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>?) {}
            override fun onAddStream(stream: MediaStream?) {}
            override fun onRemoveStream(stream: MediaStream?) {}
            override fun onDataChannel(channel: DataChannel?) { channel?.close(); channel?.dispose() }
            override fun onRenegotiationNeeded() { renegotiateLater() }
            override fun onAddTrack(receiver: RtpReceiver?, mediaStreams: Array<out MediaStream>?) {}
            override fun onTrack(transceiver: RtpTransceiver?) {
                background {
                    if (closed || applyingRemoteDescription || slots.isEmpty()) return@background
                    refreshRemoteTracks()
                    publishRemoteState()
                    picturesChanged()
                }
            }
            override fun onIceCandidate(candidate: IceCandidate?) {
                if (candidate == null) return
                background {
                    if (!closed && localIce.candidate(candidate)) emitCandidate(candidate)
                }
            }
            override fun onConnectionChange(state: PeerConnection.PeerConnectionState?) {
                background {
                    if (closed) return@background
                    val mapped = when (state) {
                        PeerConnection.PeerConnectionState.CONNECTED -> NativeMediaBackendPeerState.CONNECTED
                        PeerConnection.PeerConnectionState.DISCONNECTED -> NativeMediaBackendPeerState.RECONNECTING
                        PeerConnection.PeerConnectionState.FAILED -> NativeMediaBackendPeerState.FAILED
                        PeerConnection.PeerConnectionState.CLOSED -> NativeMediaBackendPeerState.CLOSED
                        else -> NativeMediaBackendPeerState.CONNECTING
                    }
                    emit(NativeMediaBackendEvent.PeerStateChanged(sessionId, participantId, mapped))
                    publishRemoteState()
                    picturesChanged()
                }
            }
        }
        connection = createNativeMediaPeerConnection(servers) { config ->
            core.peerConnectionFactory.createPeerConnection(config, observer)
        }
        if (initialOfferer) slots = try {
            listOf(MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO, MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO, MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO)
                .map { requireNotNull(connection.addTransceiver(it, RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.RECV_ONLY))) }
        } catch (failure: Throwable) {
            connection.dispose()
            throw failure
        }
    }

    fun setTracks(tracks: List<MediaStreamTrack?>) {
        check(!closed)
        check(tracks.size == 3)
        localTracks = tracks
        if (slots.isNotEmpty()) bindLocalTracks()
        dirty = true
    }

    private fun bindLocalTracks() {
        slots.forEachIndexed { index, slot ->
            check(slot.sender.setTrack(localTracks[index], false))
            check(slot.setDirection(if (localTracks[index] == null) RtpTransceiver.RtpTransceiverDirection.RECV_ONLY else RtpTransceiver.RtpTransceiverDirection.SEND_RECV))
        }
    }

    private fun adoptRemoteSlots() {
        // getTransceivers() disposes previously returned Java wrappers, including their
        // receiver-track handles. Detach every old sink before replacing those wrappers.
        remote.fill(null)
        picturesChanged()
        slots = emptyList()
        slots = connection.transceivers
        check(slots.size == 3)
        check(slots[0].mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO)
        check(slots[1].mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO)
        check(slots[2].mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO)
        // A commit may have changed intent while SRD was suspended. Bind the latest
        // tracks now, without waiting for readiness or holding a session transaction.
        bindLocalTracks()
        refreshRemoteTracks()
    }

    private fun refreshRemoteTracks() {
        slots.forEachIndexed { index, slot -> remote[index] = slot.receiver.track() }
    }

    suspend fun start() = observeNegotiation { ready = true; if (initialOfferer) renegotiate() }

    private suspend fun renegotiate() = negotiation.withLock {
        if (!ready || closed || !dirty || (!initialOfferer && connection.remoteDescription == null)) return@withLock
        if (connection.signalingState() != PeerConnection.SignalingState.STABLE) { dirty = true; return@withLock }
        makingOffer = true
        dirty = false
        try {
            val description = create(true)
            publishLocalDescription(description, NativeMediaDescriptionKind.OFFER)
        } finally { makingOffer = false }
    }

    suspend fun description(description: NativeMediaDescription) = observeNegotiation { negotiation.withLock {
        check(!closed)
        val offer = description.kind == NativeMediaDescriptionKind.OFFER
        val collision = offer && (makingOffer || connection.signalingState() != PeerConnection.SignalingState.STABLE)
        ignoreOffer = initialOfferer && collision
        if (ignoreOffer) return@withLock
        if (collision) {
            localIce.cancel()
            set(SessionDescription(SessionDescription.Type.ROLLBACK, ""), true)
        }
        applyingRemoteDescription = true
        try {
            set(SessionDescription(if (offer) SessionDescription.Type.OFFER else SessionDescription.Type.ANSWER, description.sdp), false)
            adoptRemoteSlots()
        } finally {
            applyingRemoteDescription = false
        }
        candidates.toList().forEach { check(connection.addIceCandidate(it)) }
        candidates.clear()
        if (offer) {
            dirty = false
            val answer = create(false)
            publishLocalDescription(answer, NativeMediaDescriptionKind.ANSWER)
        }
        publishRemoteState()
        picturesChanged()
        if (dirty) renegotiateLater()
    } }

    suspend fun candidate(value: NativeMediaIceCandidate) = observeNegotiation { negotiation.withLock {
        if (closed || ignoreOffer) return@withLock
        val candidate = IceCandidate(value.mid, value.mlineIndex?.toInt() ?: -1, value.candidate)
        if (connection.remoteDescription == null) {
            check(candidates.size < 128)
            candidates.add(candidate)
        } else check(connection.addIceCandidate(candidate))
    } }

    fun renegotiateLater() { background { renegotiate() } }

    private fun background(action: suspend () -> Unit) {
        scope.launch { observeNegotiation(action) }
    }

    private suspend fun observeNegotiation(action: suspend () -> Unit) {
        if (closed) return
        try { action() }
        catch (cancelled: kotlinx.coroutines.CancellationException) { throw cancelled }
        catch (_: Throwable) {
            if (!closed) {
                emit(NativeMediaBackendEvent.PeerStateChanged(sessionId, participantId, NativeMediaBackendPeerState.FAILED))
                close()
            }
        }
    }

    fun picture(screen: Boolean): VideoTrack? {
        val index = if (screen) 2 else 1
        return (remote[index] as? VideoTrack)?.takeIf { remoteState(index) == NativeMediaTrackState.LIVE }
    }

    private fun remoteState(index: Int): NativeMediaTrackState {
        if (slots.isEmpty()) return NativeMediaTrackState.OFF
        val direction = slots[index].currentDirection
        if (direction != RtpTransceiver.RtpTransceiverDirection.RECV_ONLY && direction != RtpTransceiver.RtpTransceiverDirection.SEND_RECV) return NativeMediaTrackState.OFF
        val track = remote[index] ?: return NativeMediaTrackState.STARTING
        return if (track.state() == MediaStreamTrack.State.LIVE && connection.connectionState() == PeerConnection.PeerConnectionState.CONNECTED) NativeMediaTrackState.LIVE else NativeMediaTrackState.INTERRUPTED
    }

    private fun publishRemoteState() {
        if (!closed) emit(NativeMediaBackendEvent.RemoteStateChanged(sessionId, participantId, NativeMediaRemoteState(remoteState(0), remoteState(1), remoteState(2))))
    }

    private suspend fun create(offer: Boolean): SessionDescription = suspendCancellableCoroutine { continuation ->
        val observer = object : SdpObserver {
            override fun onCreateSuccess(value: SessionDescription) { if (continuation.isActive) continuation.resume(value) }
            override fun onCreateFailure(error: String?) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("Media negotiation failed")) }
            override fun onSetSuccess() {}
            override fun onSetFailure(error: String?) {}
        }
        if (offer) connection.createOffer(observer, MediaConstraints()) else connection.createAnswer(observer, MediaConstraints())
    }

    private suspend fun set(value: SessionDescription, local: Boolean): Unit = suspendCancellableCoroutine { continuation ->
        val observer = object : SdpObserver {
            override fun onCreateSuccess(value: SessionDescription?) {}
            override fun onCreateFailure(error: String?) {}
            override fun onSetSuccess() { if (continuation.isActive) continuation.resume(Unit) }
            override fun onSetFailure(error: String?) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("Media negotiation failed")) }
        }
        if (local) connection.setLocalDescription(observer, value) else connection.setRemoteDescription(observer, value)
    }

    private fun emitCandidate(candidate: IceCandidate) {
        if (closed) return
        emit(NativeMediaBackendEvent.IceCandidate(sessionId, participantId,
            NativeMediaIceCandidate(candidate.sdp, candidate.sdpMid, candidate.sdpMLineIndex.takeIf { it >= 0 }?.toUShort())))
    }

    private suspend fun publishLocalDescription(description: SessionDescription, kind: NativeMediaDescriptionKind) {
        localIce.begin()
        var published = false
        try {
            set(description, true)
            currentCoroutineContext().ensureActive()
            if (closed) return
            // Install after SLD so a queued Complete from the previous ICE
            // generation cannot satisfy this generation's gathering wait.
            val complete = CompletableDeferred<Unit>()
            gatheringComplete = complete
            if (connection.iceGatheringState() != PeerConnection.IceGatheringState.COMPLETE) {
                withTimeoutOrNull(NATIVE_MEDIA_ICE_GATHERING_DEADLINE_MS) { complete.await() }
            }
            currentCoroutineContext().ensureActive()
            if (closed) return
            val current = requireNotNull(connection.localDescription)
            val late = localIce.publish(current.description)
            emit(NativeMediaBackendEvent.Description(sessionId, participantId, NativeMediaDescription(kind, current.description)))
            published = true
            late.forEach(::emitCandidate)
        } finally {
            gatheringComplete = null
            if (!published) localIce.cancel()
        }
    }

    override fun close() {
        if (closed) return
        closed = true
        gatheringComplete?.cancel()
        gatheringComplete = null
        localIce.cancel()
        candidates.clear()
        remote.fill(null)
        released()
        runCatching { picturesChanged() }
        runCatching { connection.close() }
        runCatching { connection.dispose() }
    }
}

internal inline fun <T : Any> createNativeMediaPeerConnection(
    servers: List<PeerConnection.IceServer>,
    create: (PeerConnection.RTCConfiguration) -> T?,
): T {
    val config = PeerConnection.RTCConfiguration(servers).apply {
        sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
        bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
        rtcpMuxPolicy = PeerConnection.RtcpMuxPolicy.REQUIRE
        // TURN augments direct connectivity; it is never required for Media.
        iceTransportsType = PeerConnection.IceTransportsType.ALL
        continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_CONTINUALLY
    }
    try {
        return requireNotNull(create(config))
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (failure: Exception) {
        if (servers.isEmpty()) throw failure
    }
    // Native WebRTC can reject otherwise well-formed optional server configuration.
    config.iceServers = emptyList()
    return requireNotNull(create(config))
}
