package io.paritytech.polkadotapp.feature_chain_resources_api.data.model

import androidx.annotation.Keep
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.serialization.Serializable

@Keep
@Serializable
class OnChainReservationQueueEntry(
    val account: AccountId,
    val joinedAt: Timestamp
)
