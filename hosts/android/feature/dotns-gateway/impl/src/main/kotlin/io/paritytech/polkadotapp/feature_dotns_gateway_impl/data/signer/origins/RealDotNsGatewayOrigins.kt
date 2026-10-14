package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.extension.AsDotnsGateway
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersSubscriberRepository
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealDotNsGatewayOrigins @Inject constructor(
    private val peopleMembershipProver: PeopleMembershipProver,
    private val membersSubscriberRepository: MembersSubscriberRepository,
    private val accountBytesSigner: AccountBytesSigner,
    private val chainRegistry: ChainRegistry
) : DotNsGatewayOrigins {
    override suspend fun asPersonRegistration(who: AccountId, label: String, link: DotNsLink): TransactionOrigin {
        val extension = AsDotnsGateway(
            who = who,
            label = label,
            link = link,
            peopleMembershipProver = peopleMembershipProver,
            membersSubscriberRepository = membersSubscriberRepository,
            accountBytesSigner = accountBytesSigner,
            chainRegistry = chainRegistry
        )
        return SetTransactionExtensionOrigin(TransactionSignerSource.None, extension)
    }
}
