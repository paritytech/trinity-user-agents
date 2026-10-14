package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep

@Keep
data class QueueStatusResponse(
    val queuePosition: Int,
    val group: Int,
    val estimatedIterationsRemaining: Int
)
