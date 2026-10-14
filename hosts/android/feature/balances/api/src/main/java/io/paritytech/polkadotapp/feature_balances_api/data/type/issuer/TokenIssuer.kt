package io.paritytech.polkadotapp.feature_balances_api.data.type.issuer

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.common.domain.model.AccountId

interface TokenIssuer {
    /**
     * Compose a call to issue [amount] of tokens to [destination]
     * Implementation can assume execution happens under [OriginCaller.System.Root]
     */
    suspend fun composeIssueCall(amount: Balance, destination: AccountId): GenericCall.Instance
}
