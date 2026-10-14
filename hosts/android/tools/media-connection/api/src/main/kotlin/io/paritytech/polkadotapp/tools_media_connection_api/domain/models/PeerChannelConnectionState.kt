package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

enum class PeerChannelConnectionState {
    New, Connecting, Connected, Disconnected, Failed, Closed
}

fun PeerChannelConnectionState.isTerminal(): Boolean {
    return this == PeerChannelConnectionState.Disconnected ||
        this == PeerChannelConnectionState.Failed ||
        this == PeerChannelConnectionState.Closed
}
