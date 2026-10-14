package io.paritytech.polkadotapp.feature_balances_api.data.type.external

import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.coroutines.flow.Flow

/**
 * Balance type that can be used with an arbitrary asset from the chain, given that the app knows how
 * to handle its balance type
 */
interface ExternalTokenBalanceType {
    suspend fun subscribeTransferableBalance(
        accountId: AccountId,
        subscriptionBuilder: SharedRequestsBuilder,
        externalBalanceTypeSubscriptions: ExternalBalanceTypeSubscriptions? = null,
    ): Flow<Balance>
}
