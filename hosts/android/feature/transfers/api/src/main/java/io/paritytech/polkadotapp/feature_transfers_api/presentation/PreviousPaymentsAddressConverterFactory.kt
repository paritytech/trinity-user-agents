package io.paritytech.polkadotapp.feature_transfers_api.presentation

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter

interface PreviousPaymentsAddressConverterFactory {
    fun create(chainId: ChainId): AddressConverter
}
