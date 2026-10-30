package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact

class ContactWithDevicePresence(
    val contact: Contact,
    val hasDevices: Boolean,
)
