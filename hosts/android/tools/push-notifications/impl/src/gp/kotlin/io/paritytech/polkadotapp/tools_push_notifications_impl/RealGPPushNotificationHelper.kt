package io.paritytech.polkadotapp.tools_push_notifications_impl

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.tools_push_notifications_api.NotifyType
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import io.paritytech.polkadotapp.tools_push_notifications_api.PushRule
import io.paritytech.polkadotapp.tools_push_notifications_api.SubscriptionSnapshot
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.LocalPushTokenStorage
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api.NotifyApi
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api.PushSubscriptionApi
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.NotifyRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.NotifyTypeDto
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.RuleDto
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.RulesRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.SubscriptionRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.SubscriptionResponse
import kotlinx.coroutines.flow.Flow
import timber.log.Timber
import javax.inject.Inject

class RealGPPushNotificationHelper @Inject constructor(
    private val pushTokenStorage: LocalPushTokenStorage,
    private val notifyApi: NotifyApi,
    private val subscriptionApi: PushSubscriptionApi
) : PushNotificationsHelper {
    override fun subscribeTokenChanges(): Flow<String?> {
        return pushTokenStorage.valueFlow()
    }

    override suspend fun getCurrentToken(): String? {
        return pushTokenStorage.getValue()
    }

    override suspend fun sendNotify(
        platformToken: String,
        operatingSystem: OperatingSystem,
        pushId: ByteArray,
        encryptedMessage: ByteArray,
        isVoIP: Boolean
    ): Result<Unit> = runCatching {
        val request = NotifyRequest(
            deviceToken = platformToken,
            pushId = pushId.toHexString(withPrefix = false),
            bundlerId = BuildConfig.IOS_BUNDLE_ID,
            platform = operatingSystem.toPlatformDto(),
            message = encryptedMessage.toHexString(withPrefix = false),
            voip = isVoIP
        )

        val response = notifyApi.notify(request)

        if (!response.success) {
            Timber.e("sendNotify: API returned unsuccessful, response=$response")
            error("Notify API returned unsuccessful response")
        }
    }

    override suspend fun registerSubscription(token: String): Result<SubscriptionSnapshot> = runCatching {
        subscriptionApi.register(SubscriptionRequest(token = token)).toSnapshot()
    }

    override suspend fun deleteSubscription(): Result<Unit> = runCatching {
        subscriptionApi.delete()
    }

    override suspend fun getSubscription(): Result<SubscriptionSnapshot> = runCatching {
        subscriptionApi.get().toSnapshot()
    }

    override suspend fun replaceRules(rules: List<PushRule>): Result<Unit> = runCatching {
        subscriptionApi.replaceRules(RulesRequest(rules.map { it.toDto() }))
    }

    override suspend fun addRules(rules: List<PushRule>): Result<Unit> = runCatching {
        if (rules.isEmpty()) return@runCatching

        subscriptionApi.addRules(RulesRequest(rules.map { it.toDto() }))
    }

    override suspend fun removeRules(rules: List<PushRule>): Result<Unit> = runCatching {
        if (rules.isEmpty()) return@runCatching

        subscriptionApi.removeRules(RulesRequest(rules.map { it.toDto() }))
    }

    private fun PushRule.toDto(): RuleDto = RuleDto(
        senderPubkey = senderPubKey.value.toHexString(withPrefix = false),
        topic = topic.value.toHexString(withPrefix = false),
        notifyType = notifyType.toDto()
    )

    private fun OperatingSystem.toPlatformDto(): String = when (this) {
        OperatingSystem.ANDROID -> "android"
        OperatingSystem.IOS -> "ios"
        OperatingSystem.UNKNOWN -> error("Operating system should be known to send a push notification")
    }

    private fun NotifyType.toDto(): NotifyTypeDto = when (this) {
        NotifyType.ALERT -> NotifyTypeDto.ALERT
        NotifyType.VOIP -> NotifyTypeDto.VOIP
    }

    private fun NotifyTypeDto.toDomain(): NotifyType = when (this) {
        NotifyTypeDto.ALERT -> NotifyType.ALERT
        NotifyTypeDto.VOIP -> NotifyType.VOIP
    }

    private fun RuleDto.toDomain(): PushRule = PushRule(
        senderPubKey = senderPubkey.fromHex().toDataByteArray(),
        topic = topic.fromHex().toDataByteArray(),
        notifyType = notifyType.toDomain()
    )

    private fun SubscriptionResponse.toSnapshot(): SubscriptionSnapshot = SubscriptionSnapshot(
        subscriptionId = subscriptionId,
        token = token,
        rules = rules.map { it.toDomain() }
    )
}
