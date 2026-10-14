package io.paritytech.polkadotapp.feature_usernames_api.presentation.address

import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter

interface ParseAddressUsernameConverterFactory {
    fun create(converter: AddressConverter): AddressConverter
}
