package io.paritytech.polkadotapp.feature_chats_impl.presentation.address

import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesCategory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasEstablishedChat
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_api.presentation.address.ContactsAddressConverterFactory
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class RealContactsAddressConverterFactory @Inject constructor(
    private val getContactsUseCase: GetContactsUseCase,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
) : ContactsAddressConverterFactory {
    override fun create(): AddressConverter {
        return ContactsAddressConverter(getContactsUseCase, fallbackUsernameGenerator)
    }
}

private class ContactsAddressConverter(
    private val getContactsUseCase: GetContactsUseCase,
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
) : AddressConverter {
    override suspend fun convertToAddress(input: String): ExtractedAddressesSection {
        val contacts = getContactsUseCase()
            .filter { it.hasEstablishedChat() && !it.isBlocked }
            .map { it.toExtractedAddress() }
            .filter { it.display.contains(input, ignoreCase = true) }
            .sortedBy { it.display.lowercase() }

        return ExtractedAddressesSection(
            category = ExtractedAddressesCategory.Custom(RCommon.string.search_section_contacts),
            addresses = contacts
        )
    }

    private fun Contact.toExtractedAddress(): ExtractedAddress {
        return ExtractedAddress(
            display = username ?: fallbackUsernameGenerator.generateFromAccountId(accountId),
            type = ExtractedAddress.DisplayType.USERNAME,
            accountId = accountId
        )
    }
}
