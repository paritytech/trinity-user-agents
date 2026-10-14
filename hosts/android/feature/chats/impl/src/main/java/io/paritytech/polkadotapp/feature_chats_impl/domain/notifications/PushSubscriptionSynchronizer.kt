package io.paritytech.polkadotapp.feature_chats_impl.domain.notifications

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import io.paritytech.polkadotapp.tools_push_notifications_api.PushRule
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject

internal class PushSubscriptionSynchronizer @Inject constructor(
    private val pushNotificationsHelper: PushNotificationsHelper
) {
    private val currentRules = MutableStateFlow<Set<PushRule>?>(null)

    // TODO: uncomment to enable subscription for v2 push-notifications
    suspend fun registerSubscription(token: String) {
//        currentRules.value = null
//
//        pushNotificationsHelper.registerSubscription(token)
//            .logFailure("Failed to register push subscription")
//            .onSuccess { snapshot -> currentRules.value = snapshot.rules.toSet() }
    }

    // TODO: uncomment to enable subscription for v2 push-notifications
    suspend fun syncRules(rules: List<PushRule>) {
//        val initial = currentRules.filterNotNull().first()
//        applyRuleDiff(initial, rules.toSet())
    }

    private suspend fun applyRuleDiff(initial: Set<PushRule>, target: Set<PushRule>) {
        val toAdd = (target - initial).toList()

        if (toAdd.isNotEmpty()) {
            pushNotificationsHelper.addRules(toAdd)
                .logFailure("Failed to add push rules")
                .onSuccess { currentRules.update { it.orEmpty() + toAdd } }
        }

        val toRemove = (initial - target).toList()

        if (toRemove.isNotEmpty()) {
            pushNotificationsHelper.removeRules(toRemove)
                .logFailure("Failed to remove push rules")
                .onSuccess { currentRules.update { it.orEmpty() - toRemove.toSet() } }
        }
    }
}
