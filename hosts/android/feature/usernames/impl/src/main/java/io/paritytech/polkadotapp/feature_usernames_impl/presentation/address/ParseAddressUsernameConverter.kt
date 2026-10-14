package io.paritytech.polkadotapp.feature_usernames_impl.presentation.address

import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.mapAddresses
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ResolveUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.await
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.ParseAddressUsernameConverterFactory
import javax.inject.Inject

class ParseAddressUsernameConverter(
    private val resolveUsernameUseCase: ResolveUsernamesUseCase,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val parseAddressConverter: AddressConverter,
) : AddressConverter {
    override suspend fun convertToAddress(input: String): ExtractedAddressesSection {
        val result = parseAddressConverter.convertToAddress(input)
        val ids = result.addresses.map { it.accountId }
        val usernames = resolveUsernameUseCase(ids).getOrDefault(mapOf())

        val currentUsernameWIthDomain =
            usernameOfAccountUseCase.await().username.getDisplayUsername()

        return result
            .mapAddresses {
                val accountId = it.accountId
                val usernameWithDomain = usernames[accountId]?.getDisplayUsername()

                if (usernameWithDomain == currentUsernameWIthDomain) return@mapAddresses null

                val (type, display) = if (usernameWithDomain != null) ExtractedAddress.DisplayType.USERNAME to usernameWithDomain
                else ExtractedAddress.DisplayType.ADDRESS to it.display

                ExtractedAddress(
                    display = display,
                    type = type,
                    accountId = accountId
                )
            }
    }
}

class RealParseAddressUsernameConverterFactory @Inject constructor(
    private val resolveUsernameUseCase: ResolveUsernamesUseCase,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
) : ParseAddressUsernameConverterFactory {
    override fun create(converter: AddressConverter): AddressConverter {
        return ParseAddressUsernameConverter(
            resolveUsernameUseCase,
            usernameOfAccountUseCase,
            converter
        )
    }
}
