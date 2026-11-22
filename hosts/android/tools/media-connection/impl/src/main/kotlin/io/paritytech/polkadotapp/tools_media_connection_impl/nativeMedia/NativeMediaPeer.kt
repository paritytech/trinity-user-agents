package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.webrtc.*
import uniffi.truapi.MediaRemoteState
import uniffi.truapi.MediaTrackState
import uniffi.truapi.*
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** No data channel and no application/Chat signaling. Every SDP/ICE message returns to Rust. */
internal class NativeMediaPeer(
    core: WebRtcCore,
    servers: List<PeerConnection.IceServer>,
    private val initialOfferer: Boolean,
    private val scope: CoroutineScope,
    private val emit: (MediaBackendEvent) -> Unit,
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

    internal lateinit var configuration: PeerConnection.RTCConfiguration
        private set

    init {
        val observer = object : PeerConnection.Observer {
            override fun onSignalingChange(state: PeerConnection.SignalingState?) {}
            override fun onIceConnectionChange(state: PeerConnection.IceConnectionState?) {}
            override fun onIceConnectionReceivingChange(receiving: Boolean) {}
            override fun onIceGatheringChange(state: PeerConnection.IceGatheringState?) {}
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
                    if (!closed) emit(MediaBackendEvent.IceCandidate(sessionId, participantId,
                        MediaIceCandidate(candidate.sdp, candidate.sdpMid, candidate.sdpMLineIndex.takeIf { it >= 0 }?.toUShort())))
                }
            }
            override fun onConnectionChange(state: PeerConnection.PeerConnectionState?) {
                background {
                    if (closed) return@background
                    val mapped = when (state) {
                        PeerConnection.PeerConnectionState.CONNECTED -> MediaBackendPeerState.CONNECTED
                        PeerConnection.PeerConnectionState.DISCONNECTED -> MediaBackendPeerState.RECONNECTING
                        PeerConnection.PeerConnectionState.FAILED -> MediaBackendPeerState.FAILED
                        PeerConnection.PeerConnectionState.CLOSED -> MediaBackendPeerState.CLOSED
                        else -> MediaBackendPeerState.CONNECTING
                    }
                    emit(MediaBackendEvent.PeerStateChanged(sessionId, participantId, mapped))
                    publishRemoteState()
                    picturesChanged()
                }
            }
        }
        connection = createNativeMediaPeerConnection(servers) { config ->
            configuration = config
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
            set(description, true)
            if (!closed) emit(MediaBackendEvent.Description(sessionId, participantId, MediaDescription(MediaDescriptionKind.OFFER, description.description)))
        } finally { makingOffer = false }
    }

    suspend fun description(description: MediaDescription) = observeNegotiation { negotiation.withLock {
        check(!closed)
        val offer = description.kind == MediaDescriptionKind.OFFER
        val collision = offer && (makingOffer || connection.signalingState() != PeerConnection.SignalingState.STABLE)
        ignoreOffer = initialOfferer && collision
        if (ignoreOffer) return@withLock
        if (collision) set(SessionDescription(SessionDescription.Type.ROLLBACK, ""), true)
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
            set(answer, true)
            if (!closed) emit(MediaBackendEvent.Description(sessionId, participantId, MediaDescription(MediaDescriptionKind.ANSWER, answer.description)))
        }
        publishRemoteState()
        picturesChanged()
        if (dirty) renegotiateLater()
    } }

    suspend fun candidate(value: MediaIceCandidate) = observeNegotiation { negotiation.withLock {
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
                emit(MediaBackendEvent.PeerStateChanged(sessionId, participantId, MediaBackendPeerState.FAILED))
                close()
            }
        }
    }

    fun picture(screen: Boolean): VideoTrack? {
        val index = if (screen) 2 else 1
        return (remote[index] as? VideoTrack)?.takeIf { remoteState(index) == MediaTrackState.LIVE }
    }

    private fun remoteState(index: Int): MediaTrackState {
        if (slots.isEmpty()) return MediaTrackState.OFF
        val direction = slots[index].currentDirection
        if (direction != RtpTransceiver.RtpTransceiverDirection.RECV_ONLY && direction != RtpTransceiver.RtpTransceiverDirection.SEND_RECV) return MediaTrackState.OFF
        val track = remote[index] ?: return MediaTrackState.STARTING
        return if (track.state() == MediaStreamTrack.State.LIVE && connection.connectionState() == PeerConnection.PeerConnectionState.CONNECTED) MediaTrackState.LIVE else MediaTrackState.INTERRUPTED
    }

    private fun publishRemoteState() {
        if (!closed) emit(MediaBackendEvent.RemoteStateChanged(sessionId, participantId, MediaRemoteState(remoteState(0), remoteState(1), remoteState(2))))
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

    override fun close() {
        if (closed) return
        closed = true
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
