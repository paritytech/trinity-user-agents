package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models

import androidx.annotation.Keep
import com.google.gson.JsonElement

@Keep
data class NotifyResponse(
    val errors: List<Error>?,
    val failed: Int?,
    val messageId: String?,
    val platform: String,
    val sent: Int?,
    val success: Boolean
) {
    @Keep
    data class Error(
        val device: String,
        val environment: String?,
        val response: JsonElement?,
        val status: JsonElement?
    )
}
