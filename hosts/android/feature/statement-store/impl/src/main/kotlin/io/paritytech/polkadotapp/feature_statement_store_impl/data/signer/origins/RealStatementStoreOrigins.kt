package io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.AsResourcesProofExtension
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.AsResourcesProofKind
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.notificationSlot
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
    ): Result<TransactionOrigin> = runCancellableCatching {
        val context = BandersnatchContext.statementStoreSlot(dotNsTldProvider.getTldRetrying(), period, seq)
        unsignedResourcesOrigin(AsResourcesProofKind.STATEMENT_STORE_ALLOWANCE, context, collection)
    }

    override suspend fun asResourcesNotificationSlot(
        period: UInt,
        seq: UByte,
        collection: PeopleCollection,
    ): Result<TransactionOrigin> = runCancellableCatching {
        val context = BandersnatchContext.notificationSlot(dotNsTldProvider.getTldRetrying(), period, seq)
        unsignedResourcesOrigin(AsResourcesProofKind.NOTIFICATION_FOR_COLLECTION, context, collection)
    }

    private fun unsignedResourcesOrigin(
        kind: AsResourcesProofKind,
        context: BandersnatchContext,
        collection: PeopleCollection,
    ): TransactionOrigin {
        val extension = AsResourcesProofExtension(kind, context, collection, peopleMembershipProver, chainRegistry)
        return SetTransactionExtensionOrigin(TransactionSignerSource.None, extension)
    }
}
