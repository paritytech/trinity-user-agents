package io.paritytech.polkadotapp.tools_media_connection_impl.turn

import androidx.annotation.Keep

@Keep
internal data class TurnCredentialsResponse(
    val servers: List<String>,
    val username: String,
    val password: String,
    val ttl: Int,
)
