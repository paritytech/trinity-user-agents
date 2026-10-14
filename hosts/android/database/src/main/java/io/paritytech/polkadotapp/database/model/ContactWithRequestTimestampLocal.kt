package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded

data class ContactWithRequestTimestampLocal(
    @Embedded val contact: ContactLocal,
    val requestTimestamp: Long
)
