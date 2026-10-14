package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import javax.inject.Inject

class DeviceSyncFilter @Inject constructor() {
    fun isContactSyncable(
        contact: Contact,
        walletAccount: MetaAccount
    ): Boolean = contact.isMultiDeviceChatSupported(walletAccount) && contact.establishedAt != null
}
