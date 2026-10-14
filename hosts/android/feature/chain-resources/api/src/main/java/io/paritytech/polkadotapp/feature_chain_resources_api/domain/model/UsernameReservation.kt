package io.paritytech.polkadotapp.feature_chain_resources_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlin.time.Duration

class UsernameReservation(
    val account: AccountId,
    val joinedAt: Timestamp
)

fun UsernameReservation.hasExpired(duration: Duration): Boolean {
    return System.currentTimeMillis() - duration.inWholeMilliseconds > joinedAt
}
