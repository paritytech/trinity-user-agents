package io.paritytech.polkadotapp.feature_usernames_impl.presentation.address

import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin.AddressConverter
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.SearchUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.await
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.UsernameAddressConverterFactory
import javax.inject.Inject

class UsernameAddressConverter(
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val searchUsernamesUseCase: SearchUsernamesUseCase,
) : AddressConverter {
    override suspend fun convertToAddress(input: String): ExtractedAddressesSection {
        if (input.isEmpty()) return ExtractedAddressesSection.general(emptyList())

        val currentUsername = usernameOfAccountUseCase.await().username.getDisplayUsername()
        val usernames = searchUsernamesUseCase(input)
            .map {
                it.filter { it.username.getDisplayUsername() != currentUsername }
            }
            .mapList {
                ExtractedAddress(
                    display = it.username.getDisplayUsername(),
                    accountId = it.accountId,
                    type = ExtractedAddress.DisplayType.USERNAME
                )
            }
            .getOrNull()

        return ExtractedAddressesSection.general(usernames ?: emptyList())
    }
}

class RealUsernameAddressConverterFactory @Inject constructor(
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val searchUsernamesUseCase: SearchUsernamesUseCase,
) : UsernameAddressConverterFactory {
    override fun create(): AddressConverter {
        return UsernameAddressConverter(
            usernameOfAccountUseCase = usernameOfAccountUseCase,
            searchUsernamesUseCase = searchUsernamesUseCase,
        )
    }
}
