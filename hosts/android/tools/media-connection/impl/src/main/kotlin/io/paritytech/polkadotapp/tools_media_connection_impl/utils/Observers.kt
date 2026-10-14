package io.paritytech.polkadotapp.tools_media_connection_impl.utils

import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.RtpReceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import java.util.concurrent.CopyOnWriteArrayList

abstract class SimplePeerConnectionObserver : PeerConnection.Observer {
    override fun onSignalingChange(state: PeerConnection.SignalingState) = Unit

    override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit

    override fun onIceConnectionReceivingChange(p0: Boolean) = Unit

    override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) = Unit

    override fun onIceCandidate(iceCandidate: IceCandidate) = Unit

    override fun onIceCandidatesRemoved(iceCandidates: Array<out IceCandidate>) = Unit

    override fun onAddStream(stream: MediaStream) = Unit

    override fun onRemoveStream(stream: MediaStream) = Unit

    override fun onDataChannel(channel: DataChannel) = Unit

    override fun onRenegotiationNeeded() = Unit

    override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream?>) = Unit

    override fun onConnectionChange(newState: PeerConnection.PeerConnectionState) = Unit
}

abstract class SimpleSdpObserver : SdpObserver {
    override fun onCreateSuccess(sdp: SessionDescription) = Unit

    override fun onSetSuccess() = Unit

    override fun onCreateFailure(error: String) = Unit

    override fun onSetFailure(error: String) = Unit
}

class CompoundPeerConnectionObserver : PeerConnection.Observer {
    private val observers = CopyOnWriteArrayList<PeerConnection.Observer>()

    fun addObserver(observer: PeerConnection.Observer) {
        observers.add(observer)
    }

    fun removeObserver(observer: PeerConnection.Observer) {
        observers.remove(observer)
    }

    override fun onSignalingChange(state: PeerConnection.SignalingState) {
        observers.forEach { it.onSignalingChange(state) }
    }

    override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) {
        observers.forEach { it.onIceConnectionChange(state) }
    }

    override fun onIceConnectionReceivingChange(p0: Boolean) {
        observers.forEach { it.onIceConnectionReceivingChange(p0) }
    }

    override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) {
        observers.forEach { it.onIceGatheringChange(state) }
    }

    override fun onIceCandidate(iceCandidate: IceCandidate) {
        observers.forEach { it.onIceCandidate(iceCandidate) }
    }

    override fun onIceCandidatesRemoved(iceCandidates: Array<out IceCandidate>) {
        observers.forEach { it.onIceCandidatesRemoved(iceCandidates) }
    }

    override fun onAddStream(stream: MediaStream) {
        observers.forEach { it.onAddStream(stream) }
    }

    override fun onRemoveStream(stream: MediaStream) {
        observers.forEach { it.onRemoveStream(stream) }
    }

    override fun onDataChannel(channel: DataChannel) {
        observers.forEach { it.onDataChannel(channel) }
    }

    override fun onRenegotiationNeeded() {
        observers.forEach { it.onRenegotiationNeeded() }
    }

    override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream?>) {
        observers.forEach { it.onAddTrack(receiver, mediaStreams) }
    }

    override fun onConnectionChange(newState: PeerConnection.PeerConnectionState) {
        observers.forEach { it.onConnectionChange(newState) }
    }
}
