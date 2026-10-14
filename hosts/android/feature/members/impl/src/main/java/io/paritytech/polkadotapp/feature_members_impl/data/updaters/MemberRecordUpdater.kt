package io.paritytech.polkadotapp.feature_members_impl.data.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.updaters.SingleStorageKeyUpdater
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMemberKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api.members

internal class MemberRecordUpdater(
    override val scope: Updater.NoChainScope<MetaAccount>,
    private val collectionId: RingCollectionId,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    chainRegistry: ChainRegistry,
    storageCache: StorageCache,
) : SingleStorageKeyUpdater<MetaAccount>(
    scope = scope,
    chainRegistry = chainRegistry,
    storageCache = storageCache
) {
    context(withRuntime: WithRuntime)
    override suspend fun storageKey(
        scopeValue: MetaAccount,
        chain: Chain,
    ): String {
        val personKey = bandersnatchSecretsStorage.getMemberKey(scopeValue.id)
        return withRuntime.runtime.metadata.members.members.storageKey(collectionId, personKey)
    }
}
