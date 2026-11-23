package io.paritytech.polkadotapp.tools_media_connection_impl.models

import org.webrtc.PeerConnection

internal data class ExternalRtcConfig(
    val turnCredentials: List<TurnCredentials>,
)

internal data class TurnCredentials(
    val url: String,
    val username: String,
    val password: String,
)

internal fun ExternalRtcConfig.toIceServers(): List<PeerConnection.IceServer> = listOf(
    PeerConnection.IceServer.builder(listOf(
        "stun:stun.l.google.com:19302",
        "stun:stun1.l.google.com:19302",
        "stun:stun2.l.google.com:19302",
        "stun:stun3.l.google.com:19302",
        "stun:stun4.l.google.com:19302",
    )).createIceServer(),
) + turnCredentials.map { turn ->
    PeerConnection.IceServer.builder(turn.url)
        .setUsername(turn.username)
        .setPassword(turn.password)
        .createIceServer()
}
