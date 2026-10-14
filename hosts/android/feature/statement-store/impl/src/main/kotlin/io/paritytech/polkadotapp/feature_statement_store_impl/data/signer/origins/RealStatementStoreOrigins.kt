package io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.RegisterStatementStoreAllowance
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.statementStoreSlot
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealStatementStoreOrigins @Inject constructor(
    private val peopleMembershipProver: PeopleMembershipProver,
    private val chainRegistry: ChainRegistry,
    private val dotNsTldProvider: DotNsTldProvider,
) : StatementStoreOrigins {
    override suspend fun asResourcesStatementStoreSlot(
        period: UInt,
        seq: UInt,
        collection: PeopleCollection,
    ): Result<TransactionOrigin> {
        return runCatching {
            val extension = RegisterStatementStoreAllowance(
                context = BandersnatchContext.statementStoreSlot(dotNsTldProvider.getTldRetrying(), period, seq),
                collection = collection,
                peopleMembershipProver = peopleMembershipProver,
                chainRegistry = chainRegistry,
            )
            SetTransactionExtensionOrigin(TransactionSignerSource.None, extension)
        }
    }
}
