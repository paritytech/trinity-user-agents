package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

// TODO this should be in the people related feature module
interface FreeTransactionOrigins {
    suspend fun freeTxFromWalletOrSigned(chainId: ChainId): TransactionOrigin
}
