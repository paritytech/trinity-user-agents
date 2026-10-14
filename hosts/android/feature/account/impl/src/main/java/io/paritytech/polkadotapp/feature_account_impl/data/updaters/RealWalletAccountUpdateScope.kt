package io.paritytech.polkadotapp.feature_account_impl.data.updaters

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealWalletAccountUpdateScope @Inject constructor(
    private val accountRepository: AccountRepository
) : Updater.NoChainScope<MetaAccount> {
    override fun invalidationFlow(): Flow<MetaAccount> {
        return accountRepository.walletAccountFlow()
    }
}
