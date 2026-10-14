package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

class HopPoolStatus(
    val entryCount: Long,
    val totalBytes: Long,
    val maxBytes: Long
)

class HopSubmitResult(
    val poolStatus: HopPoolStatus
)
