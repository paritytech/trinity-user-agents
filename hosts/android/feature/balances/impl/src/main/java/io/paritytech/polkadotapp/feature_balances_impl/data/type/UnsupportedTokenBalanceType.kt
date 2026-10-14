package io.paritytech.polkadotapp.feature_balances_impl.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceType
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class UnsupportedTokenBalanceType @Inject constructor() : TokenBalanceType {
    override suspend fun minimumBalance(): Balance {
        return Balance.ZERO
    }

    override suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*> {
        return emptyFlow<Nothing>()
    }

    override suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder?
    ): Flow<AccountBalanceUpdate> {
        return emptyFlow()
    }

    override suspend fun getBalance(accountId: AccountId): TokenBalance {
        error("Cannot fetch balance for an unsupported token type")
    }

    override suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean {
        return false
    }

    override fun isSelfSufficient(): Boolean {
        return false
    }
}
