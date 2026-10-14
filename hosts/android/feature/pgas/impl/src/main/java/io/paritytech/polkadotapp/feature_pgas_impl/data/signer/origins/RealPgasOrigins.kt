package io.paritytech.polkadotapp.feature_pgas_impl.data.signer.origins

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersSubscriberRepository
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_pgas_impl.data.extension.AsPgas
import io.paritytech.polkadotapp.feature_pgas_impl.data.extension.pgasClaim
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealPgasOrigins @Inject constructor(
    private val peopleMembershipProver: PeopleMembershipProver,
    private val membersRepository: MembersRepository,
    private val membersSubscriberRepository: MembersSubscriberRepository,
    private val chainRegistry: ChainRegistry,
    private val chainStateRepository: ChainStateRepository,
    private val dotNsTldProvider: DotNsTldProvider,
) : PgasOrigins {
    override suspend fun asPgasClaim(
        period: UInt,
        slotIndex: UInt,
        collection: PeopleCollection,
    ): Result<TransactionOrigin> {
        return runCatching {
            val extension = AsPgas(
                period = period,
                context = BandersnatchContext.pgasClaim(dotNsTldProvider.getTldRetrying(), period, slotIndex),
                collection = collection,
                peopleMembershipProver = peopleMembershipProver,
                membersRepository = membersRepository,
                membersSubscriberRepository = membersSubscriberRepository,
                chainRegistry = chainRegistry,
                chainStateRepository = chainStateRepository,
            )
            SetTransactionExtensionOrigin(TransactionSignerSource.None, extension)
        }
    }
}
