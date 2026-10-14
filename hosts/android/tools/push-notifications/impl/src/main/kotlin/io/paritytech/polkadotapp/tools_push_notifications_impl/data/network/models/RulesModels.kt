package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName

@Keep
data class RuleDto(
    @SerializedName("id") val id: String? = null,
    @SerializedName("sender_pubkey") val senderPubkey: String,
    @SerializedName("topic") val topic: String,
    @SerializedName("notify_type") val notifyType: NotifyTypeDto
)

@Keep
data class RulesRequest(
    @SerializedName("rules") val rules: List<RuleDto>
)

@Keep
data class ReplaceRulesResponse(
    @SerializedName("rules_count") val rulesCount: Int,
    @SerializedName("rules") val rules: List<RuleDto>
)

@Keep
data class AddRulesResponse(
    @SerializedName("added") val added: Int,
    @SerializedName("total_rules") val totalRules: Int
)

@Keep
data class RemoveRulesResponse(
    @SerializedName("removed") val removed: Int,
    @SerializedName("total_rules") val totalRules: Int
)

enum class NotifyTypeDto {
    @SerializedName("alert")
    ALERT,

    @SerializedName("voip")
    VOIP
}
