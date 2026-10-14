package io.paritytech.polkadotapp.feature_account_api.presentation.address.converter

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin

interface ParseAddressConverterFactory {
    fun create(chainId: ChainId): AddressInputMixin.AddressConverter
}
