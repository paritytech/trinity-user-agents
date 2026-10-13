package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

class GetAddressUseCase @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val digitalDollarAssetProvider: ChainAssetProvider,
    private val accountRepository: AccountRepository,
) {
    suspend fun wallet(): String {
        val chain = digitalDollarAssetProvider.chain()
        return accountRepository.getWalletAccount().addressIn(chain)
    }
}
