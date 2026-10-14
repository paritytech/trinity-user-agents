package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName

@Keep
data class SubscriptionResponse(
    @SerializedName("id") val subscriptionId: String,
    @SerializedName("client_pubkey") val clientPubkey: String?,
    @SerializedName("platform") val platform: String,
    @SerializedName("fcm_token") val token: String?,
    @SerializedName("rules") val rules: List<RuleDto>,
    @SerializedName("created_at") val createdAt: String?
)
