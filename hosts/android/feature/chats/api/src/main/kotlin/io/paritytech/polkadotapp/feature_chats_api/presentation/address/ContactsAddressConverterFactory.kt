package io.paritytech.polkadotapp.feature_chats_api.presentation.address

import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter

interface ContactsAddressConverterFactory {
    fun create(): AddressConverter
}
