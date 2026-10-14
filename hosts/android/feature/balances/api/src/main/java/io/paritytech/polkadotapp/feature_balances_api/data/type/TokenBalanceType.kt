package io.paritytech.polkadotapp.feature_balances_api.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.domain.model.AccountBalanceUpdate
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

/**
 * Balance type that can be used when asset is directly known to the app: there exists a
 * [Chain.Asset] in config
 */
interface TokenBalanceType {
    /**
     * Minimum balance that account required to have
     *
     * Typically after dropping below minimum balance the balance of counted is dusted to zero.
     * So its not possible to have balance from the open interval (0, minimum_balance)
     *
     * Dusting usually also involves decrementing sufficient reference thus potentially removing account as a whole if it was the latest sufficient reference
     */
    suspend fun minimumBalance(): Balance

    /**
     * Start syncing balances of a [metaAccount].
     * Syncing is done by writing to the local database.
     */
    suspend fun startSyncingBalance(
        metaAccount: MetaAccount,
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<*>

    suspend fun subscribeAccountBalanceUpdates(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder? = null
    ): Flow<AccountBalanceUpdate>

    suspend fun getBalance(accountId: AccountId): TokenBalance

    suspend fun totalCanDropBelowMinimumBalance(accountId: AccountId): Boolean

    fun isSelfSufficient(): Boolean
}

suspend fun TokenBalanceType.subscribeAccountBalance(
    accountId: AccountId,
    subscriptionBuilder: SharedRequestsBuilder? = null
): Flow<TokenBalance> {
    return subscribeAccountBalanceUpdates(accountId, subscriptionBuilder).map { it.balance }
}
