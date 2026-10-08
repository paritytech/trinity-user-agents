package io.paritytech.polkadotapp.tools_push_notifications_impl

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import io.paritytech.polkadotapp.tools_push_notifications_api.PushRule
import io.paritytech.polkadotapp.tools_push_notifications_api.SubscriptionSnapshot
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealVanillaPushNotificationHelper @Inject constructor() : PushNotificationsHelper {
    override fun subscribeTokenChanges(): Flow<String?> {
        return flowOf { null }
    }

    override suspend fun getCurrentToken(): String? {
        return null
    }

    override suspend fun sendNotify(
        platformToken: String,
        operatingSystem: OperatingSystem,
        pushId: ByteArray,
        encryptedMessage: ByteArray,
        isVoIP: Boolean
    ): Result<Unit> = failure()

    override suspend fun registerSubscription(token: String): Result<SubscriptionSnapshot> = failure()

    override suspend fun deleteSubscription(): Result<Unit> = failure()

    override suspend fun getSubscription(): Result<SubscriptionSnapshot> = failure()

    override suspend fun replaceRules(rules: List<PushRule>): Result<Unit> = failure()

    override suspend fun addRules(rules: List<PushRule>): Result<Unit> = failure()

    override suspend fun removeRules(rules: List<PushRule>): Result<Unit> = failure()

    private fun <T> failure() = Result.failure<T>(Throwable("Not Implemented for vanilla flavour"))

}
