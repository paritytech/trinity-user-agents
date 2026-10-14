package io.paritytech.polkadotapp.feature_chats_api.domain.model

data class ContactWithRequestTimestamp(
    val contact: Contact,
    val requestTimestamp: Long
)
