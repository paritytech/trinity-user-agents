package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection

data class NotificationSlot(
    val collection: PeopleCollection,
    val period: UInt,
    val seq: UByte,
)
