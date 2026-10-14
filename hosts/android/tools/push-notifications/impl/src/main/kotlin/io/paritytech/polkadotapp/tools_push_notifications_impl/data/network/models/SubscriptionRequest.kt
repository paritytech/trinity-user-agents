package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName

@Keep
data class SubscriptionRequest(
    @SerializedName("platform") val platform: String = "android",
    @SerializedName("fcm_token") val token: String
)
