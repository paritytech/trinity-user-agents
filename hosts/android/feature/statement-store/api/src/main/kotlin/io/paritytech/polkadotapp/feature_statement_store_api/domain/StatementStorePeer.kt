package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import kotlinx.coroutines.flow.Flow

interface StatementStorePeer {
    /** The chain whose socket carries statement-store traffic; the store opens none of its own. */
    val chainId: ChainId

    /**
     * Whether the peer has answered any subscription [StatementStoreService] currently holds. It never
     * reports that the peer went away: the socket silently re-sends subscribe requests on reconnect, so a
     * subscription that stopped being served looks exactly like a quiet one.
     */
    fun observeAnswered(): Flow<Boolean>
}
