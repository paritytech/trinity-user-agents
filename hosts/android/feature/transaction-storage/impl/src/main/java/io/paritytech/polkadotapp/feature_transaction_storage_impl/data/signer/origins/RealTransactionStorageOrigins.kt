package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.signer.origins

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.extension.ClaimLongTermStorage
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.extension.longTermStorageClaim
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealTransactionStorageOrigins @Inject constructor(
    private val peopleMembershipProver: PeopleMembershipProver,
    private val chainRegistry: ChainRegistry,
    private val dotNsTldProvider: DotNsTldProvider,
) : TransactionStorageOrigins {
    override suspend fun asResourcesLongTermStorage(
        period: UInt,
        counter: UByte,
        collection: PeopleCollection,
    ): Result<TransactionOrigin> {
        return runCatching {
            val extension = ClaimLongTermStorage(
                context = BandersnatchContext.longTermStorageClaim(dotNsTldProvider.getTldRetrying(), period, counter),
                collection = collection,
                peopleMembershipProver = peopleMembershipProver,
                chainRegistry = chainRegistry,
            )
            SetTransactionExtensionOrigin(TransactionSignerSource.None, extension)
        }
    }
}
