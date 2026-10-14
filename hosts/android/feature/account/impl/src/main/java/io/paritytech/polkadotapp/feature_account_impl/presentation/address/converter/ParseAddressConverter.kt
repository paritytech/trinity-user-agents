package io.paritytech.polkadotapp.feature_account_impl.presentation.address.converter

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.accountIdOf
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.feature_account_api.presentation.address.converter.ParseAddressConverterFactory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import javax.inject.Inject

class ParseAddressConverter(
    private val chainId: ChainId,
    private val chainRegistry: ChainRegistry,
) : AddressConverter {
    override suspend fun convertToAddress(input: String): ExtractedAddressesSection {
        val chain = chainRegistry.getChain(chainId)
        val accountId = chain.accountIdOf(input)
        val addresses = ExtractedAddress(
            display = chain.addressOf(accountId),
            type = ExtractedAddress.DisplayType.ADDRESS,
            accountId = accountId
        )

        return ExtractedAddressesSection.general(listOf(addresses))
    }
}

class RealParseAddressConverterFactory @Inject constructor(
    private val chainRegistry: ChainRegistry,
) : ParseAddressConverterFactory {
    override fun create(chainId: ChainId): AddressConverter {
        return ParseAddressConverter(chainId, chainRegistry)
    }
}
