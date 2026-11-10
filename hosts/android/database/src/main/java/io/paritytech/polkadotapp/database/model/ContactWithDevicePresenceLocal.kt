package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded

class ContactWithDevicePresenceLocal(
    @Embedded val contact: ContactLocal,
    val hasDevices: Boolean,
)
