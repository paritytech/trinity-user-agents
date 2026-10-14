package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact

fun Contact.isMultiDeviceChatSupported(metaAccount: MetaAccount): Boolean {
    if (metaAccount.purpose != MetaAccount.Purpose.WALLET) return false
    return this.ourMetaAccountId == metaAccount.id
}
