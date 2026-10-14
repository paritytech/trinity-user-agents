package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName

@Keep
data class UsernameClaimResponse(
    @SerializedName("base_username")
    val baseUsername: String?,
    val digits: String?,
    val username: String?,
    val registrationOutcome: RegistrationOutcome?
)

enum class RegistrationOutcome {
    @SerializedName("QUEUED")
    QUEUED,

    @SerializedName("PAYMENT_REQUIRED")
    PAYMENT_REQUIRED
}
