package io.paritytech.polkadotapp.tools_push_notifications_api

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.coroutines.flow.Flow

interface PushNotificationsHelper {
    fun subscribeTokenChanges(): Flow<String?>
    suspend fun getCurrentToken(): String?
    suspend fun sendNotify(
        platformToken: String,
        operatingSystem: OperatingSystem,
        pushId: ByteArray,
        encryptedMessage: ByteArray,
        isVoIP: Boolean = false
    ): Result<Unit>

    suspend fun registerSubscription(token: String): Result<SubscriptionSnapshot>
    suspend fun deleteSubscription(): Result<Unit>
    suspend fun getSubscription(): Result<SubscriptionSnapshot>
    suspend fun replaceRules(rules: List<PushRule>): Result<Unit>
    suspend fun addRules(rules: List<PushRule>): Result<Unit>
    suspend fun removeRules(rules: List<PushRule>): Result<Unit>
}

data class PushRule(
    val senderPubKey: AccountId,
    val topic: DataByteArray,
    val notifyType: NotifyType
)

enum class NotifyType { ALERT, VOIP }

data class SubscriptionSnapshot(
    val subscriptionId: String,
    val token: String?,
    val rules: List<PushRule>
)
