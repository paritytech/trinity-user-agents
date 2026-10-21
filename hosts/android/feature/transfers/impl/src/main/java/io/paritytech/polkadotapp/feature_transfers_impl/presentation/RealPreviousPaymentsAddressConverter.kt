package io.paritytech.polkadotapp.feature_transfers_impl.presentation

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesCategory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import io.paritytech.polkadotapp.feature_transfers_api.domain.usecase.PreviousSendRecipientsUseCase
import io.paritytech.polkadotapp.feature_transfers_api.presentation.PreviousPaymentsAddressConverterFactory
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class RealPreviousPaymentsAddressConverterFactory @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val sendRecipientsUseCase: PreviousSendRecipientsUseCase,
) : PreviousPaymentsAddressConverterFactory {
    override fun create(chainId: ChainId): AddressInputMixin.AddressConverter {
        return PreviousPaymentsAddressConverter(chainId, chainRegistry, sendRecipientsUseCase)
    }
}

private class PreviousPaymentsAddressConverter(
    private val chainId: ChainId,
    private val chainRegistry: ChainRegistry,
    private val sendRecipientsUseCase: PreviousSendRecipientsUseCase,
) : AddressInputMixin.AddressConverter {
    private var cache: List<ExtractedAddress>? = null
    private val mutex = Mutex()

    override suspend fun convertToAddress(input: String): ExtractedAddressesSection {
        val relevantPreviousPayments = getAllPreviousPayments()
            .filter { it.display.contains(input, ignoreCase = true) }

        return ExtractedAddressesSection(
            category = ExtractedAddressesCategory.Custom(RCommon.string.search_section_recent),
            addresses = relevantPreviousPayments
        )
    }

    private suspend fun getAllPreviousPayments(): List<ExtractedAddress> {
        return mutex.withLock {
            cache ?: fetchAllPreviousPayments().also { cache = it }
        }
    }

    private suspend fun fetchAllPreviousPayments(): List<ExtractedAddress> {
        val chain = chainRegistry.getChain(chainId)

        return sendRecipientsUseCase().getOrEmpty()
            .sortedByDescending { it.createdAt }
            .distinctBy { it.accountId }
            .map {
                val label = it.label

                if (label == null) {
                    ExtractedAddress(
                        display = chain.addressOf(it.accountId),
                        accountId = it.accountId,
                        type = ExtractedAddress.DisplayType.ADDRESS,
                    )
                } else {
                    ExtractedAddress(
                        display = label,
                        accountId = it.accountId,
                        type = ExtractedAddress.DisplayType.USERNAME,
                    )
                }
            }
    }
}
