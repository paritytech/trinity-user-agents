package io.paritytech.polkadotapp.feature_chain_resources_api.data.model

import kotlinx.serialization.Serializable

@Serializable
class NotificationReference(
    val period: UInt,
    val seq: UByte,
)
