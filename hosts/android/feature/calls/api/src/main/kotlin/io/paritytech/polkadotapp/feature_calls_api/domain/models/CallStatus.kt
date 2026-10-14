package io.paritytech.polkadotapp.feature_calls_api.domain.models

import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import kotlin.time.Duration

sealed interface CallStatus {
    data object Requesting : CallStatus
    data object Ringing : CallStatus
    data object Connecting : CallStatus
    data class Connected(val duration: Duration) : CallStatus
    data object Ended : CallStatus
    data object Failed : CallStatus
}

fun CallStatus.isTerminal(): Boolean = this is CallStatus.Ended || this is CallStatus.Failed

data class ActiveCallState(
    val chatId: ChatId,
    val offerId: OfferId,
    val direction: CallDirection,
    val status: CallStatus,
    val initiatedWithVideo: Boolean,
)
