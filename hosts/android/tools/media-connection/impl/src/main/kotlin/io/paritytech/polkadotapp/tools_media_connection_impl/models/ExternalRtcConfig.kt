package io.paritytech.polkadotapp.tools_media_connection_impl.models

internal data class ExternalRtcConfig(
    val turnCredentials: List<TurnCredentials>,
)

internal data class TurnCredentials(
    val url: String,
    val username: String,
    val password: String,
)
