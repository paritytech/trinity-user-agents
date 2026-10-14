package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName

/**
 * Versioned response from `POST /api/v1/usernames/available?version=v1`.
 * Shape: `{"_tag":"v1","value":{"alice":{"status":"AVAILABLE","availableDigits":[11,23,42]}}}`
 */
@Keep
data class UsernameAvailableResponse(
    @SerializedName("_tag")
    val tag: String,
    val value: Map<String, UsernameAvailableEntry>,
)

@Keep
data class UsernameAvailableEntry(
    val status: String,
    val availableDigits: List<Int>?,
)
