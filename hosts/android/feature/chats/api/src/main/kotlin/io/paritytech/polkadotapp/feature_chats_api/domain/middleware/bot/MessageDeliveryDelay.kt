package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import kotlinx.coroutines.delay
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

enum class MessageDeliveryDelay {
    IMMEDIATE, HUMAN_INTERACTION
}

suspend fun MessageDeliveryDelay.delayDelivery() {
    val delayDuration = when (this) {
        MessageDeliveryDelay.IMMEDIATE -> Duration.ZERO
        MessageDeliveryDelay.HUMAN_INTERACTION -> 500.milliseconds
    }

    delay(delayDuration)
}
