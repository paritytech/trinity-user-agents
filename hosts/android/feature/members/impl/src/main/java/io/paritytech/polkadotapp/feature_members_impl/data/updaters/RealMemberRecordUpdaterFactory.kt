package io.paritytech.polkadotapp.feature_members_impl.data.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.updaters.MemberRecordUpdaterFactory
import javax.inject.Inject

internal class RealMemberRecordUpdaterFactory @Inject constructor(
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val chainRegistry: ChainRegistry,
    private val storageCache: StorageCache,
) : MemberRecordUpdaterFactory {
    override fun create(
        scope: Updater.NoChainScope<MetaAccount>,
        collectionId: RingCollectionId,
    ): Updater<MetaAccount> {
        return MemberRecordUpdater(
            scope = scope,
            collectionId = collectionId,
            bandersnatchSecretsStorage = bandersnatchSecretsStorage,
            chainRegistry = chainRegistry,
            storageCache = storageCache,
        )
    }
}
