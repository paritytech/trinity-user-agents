package io.paritytech.polkadotapp.tools_media_connection_impl.utils

import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoDegradationPreference
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoEncodingProfile
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeout
import org.webrtc.*
import kotlin.coroutines.Continuation
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.time.Duration.Companion.seconds

fun RtpSender.applyEncodingProfile(profile: VideoEncodingProfile) {
    val currentParameters = parameters
    if (currentParameters == null || currentParameters.encodings.isEmpty()) return

    currentParameters.encodings.forEach { encoding ->
        if (profile.maxBitrateBps != VideoEncodingProfile.UNLIMITED_BITRATE_BPS) {
            encoding.maxBitrateBps = profile.maxBitrateBps
        }
    }
    currentParameters.degradationPreference = profile.degradationPreference.toRtpDegradationPreference()

    parameters = currentParameters
}

private fun VideoDegradationPreference.toRtpDegradationPreference(): RtpParameters.DegradationPreference = when (this) {
    VideoDegradationPreference.MAINTAIN_FRAMERATE -> RtpParameters.DegradationPreference.MAINTAIN_FRAMERATE
    VideoDegradationPreference.MAINTAIN_RESOLUTION -> RtpParameters.DegradationPreference.MAINTAIN_RESOLUTION
    VideoDegradationPreference.BALANCED -> RtpParameters.DegradationPreference.BALANCED
    VideoDegradationPreference.DISABLED -> RtpParameters.DegradationPreference.DISABLED
}

fun PeerConnection.preferVideoCodec(peerConnectionFactory: PeerConnectionFactory, codecName: String) {
    val videoTransceiver = transceivers.firstOrNull {
        it.mediaType == MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO
    } ?: return

    val capabilities = peerConnectionFactory.getRtpSenderCapabilities(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO)
    val preferred = capabilities.codecs.filter { it.name.equals(codecName, ignoreCase = true) }
    if (preferred.isEmpty()) return

    val rest = capabilities.codecs.filterNot { it.name.equals(codecName, ignoreCase = true) }
    videoTransceiver.setCodecPreferences(preferred + rest)
}

suspend fun PeerConnection.createOffer() = suspendCancellableCoroutine {
    createOffer(
        onCreateSdpObserver(SessionDescription.Type.OFFER, it),
        MediaConstraints()
    )
}

suspend fun PeerConnection.createAnswer() = suspendCancellableCoroutine {
    createAnswer(
        onCreateSdpObserver(SessionDescription.Type.ANSWER, it),
        MediaConstraints()
    )
}

suspend fun PeerConnection.setLocalDescription(sdp: SessionDescription) = suspendCancellableCoroutine {
    setLocalDescription(
        onSetSdpObserver(sdp.type, it),
        sdp
    )
}

suspend fun PeerConnection.setRemoteDescription(
    sdp: SessionDescription
) = suspendCancellableCoroutine { continuation ->
    setRemoteDescription(
        onSetSdpObserver(sdp.type, continuation),
        sdp
    )
}

suspend fun PeerConnection.addCandidate(candidate: IceCandidate) = suspendCancellableCoroutine { continuation ->
    addIceCandidate(
        candidate,
        onAddCandidateObserver(continuation)
    )
}

suspend fun PeerConnection.addCandidates(candidates: List<IceCandidate>) {
    candidates.forEach { addCandidate(it) }
}

suspend fun PeerConnection.awaitRemoteSdpSet() {
    withTimeout(10.seconds) {
        while (remoteDescription == null) {
            delay(100)
        }
    }
}

private fun onCreateSdpObserver(type: SessionDescription.Type, continuation: Continuation<SessionDescription>) = object : SimpleSdpObserver() {
    override fun onCreateSuccess(sdp: SessionDescription) {
        continuation.resume(sdp)
    }

    override fun onCreateFailure(error: String) {
        continuation.resumeWithException(RuntimeException("Failed to create SDP $type: $error"))
    }
}

private fun onSetSdpObserver(type: SessionDescription.Type, continuation: Continuation<Unit>): SimpleSdpObserver = object : SimpleSdpObserver() {
    override fun onSetSuccess() {
        continuation.resume(Unit)
    }

    override fun onSetFailure(error: String) {
        continuation.resumeWithException(Exception("Failed to set SDP $type: $error"))
    }
}

private fun onAddCandidateObserver(continuation: Continuation<Unit>) = object : AddIceObserver {
    override fun onAddSuccess() {
        continuation.resume(Unit)
    }

    override fun onAddFailure(error: String) {
        continuation.resumeWithException(Exception("Failed to add ICE candidate: $error"))
    }
}
