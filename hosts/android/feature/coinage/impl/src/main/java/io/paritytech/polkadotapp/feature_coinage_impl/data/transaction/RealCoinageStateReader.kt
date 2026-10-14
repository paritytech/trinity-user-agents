package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.StorageKey4
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.ensureKeysWithNullDefault
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRingCollectionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinsByOwner
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.recyclerAliasStates
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.recyclersCoinToRecycler
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import javax.inject.Inject

class RealCoinageStateReaderFactory @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    @param:RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val membersRepository: MembersRepository,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : CoinageStateReaderFactory {
    /**
     * The instance id is read once per pass rather than per transaction: it keys every coinage storage
     * lookup below, and a pass that could not resolve it can read nothing at all.
     */
    override suspend fun create(view: PinnedChainView): CoinageStateReader = RealCoinageStateReader(
        chainId = chainAssetProvider.chainId(),
        instanceId = coinageInstanceIdProvider.instanceId().getOrThrow(),
        remoteStorageSource = remoteStorageSource,
        membersRepository = membersRepository,
    )
}

private class RealCoinageStateReader(
    private val chainId: ChainId,
    private val instanceId: CoinageInstanceId,
    private val remoteStorageSource: StorageDataSource,
    private val membersRepository: MembersRepository,
) : CoinageStateReader {
    override suspend fun coinsAt(at: BlockHash, coins: List<AccountId>): Result<Map<AccountId, OnChainCoinInfo?>> {
        if (coins.isEmpty()) return Result.success(emptyMap())

        return remoteStorageSource.queryCatching(chainId, at = at) {
            metadata.coinage.coinsByOwner.entries(coins)
        }.map { it.ensureKeysWithNullDefault(coins) }
    }

    override suspend fun recyclerMembershipsAt(
        at: BlockHash,
        memberKeys: List<BandersnatchPublicKey>,
    ): Result<Map<BandersnatchPublicKey, ValueExponent?>> {
        if (memberKeys.isEmpty()) return Result.success(emptyMap())

        return remoteStorageSource.queryCatching(chainId, at = at) {
            metadata.coinage.recyclersCoinToRecycler.entries(memberKeys)
        }.map { locations ->
            locations
                .filterValues { location -> location.instanceId.toUInt() == instanceId }
                .mapValues { (_, location) -> ValueExponent(location.value) }
                .ensureKeysWithNullDefault(memberKeys)
        }
    }

    override suspend fun ringPositionsAt(
        at: BlockHash,
        memberships: Map<BandersnatchPublicKey, ValueExponent>,
    ): Result<Map<BandersnatchPublicKey, RingPosition?>> {
        if (memberships.isEmpty()) return Result.success(emptyMap())

        val keys = memberships.map { (member, denomination) -> denomination.toRingCollectionId(instanceId) to member }

        return membersRepository.fetchMembers(
            chainId = chainId,
            keys = keys,
            consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
            blockHash = at,
        ).map { positions ->
            positions.mapKeys { (key, _) -> key.second }.ensureKeysWithNullDefault(memberships.keys)
        }
    }

    override suspend fun aliasStatesAt(
        at: BlockHash,
        keys: List<RecyclerAliasKey>,
    ): Result<Map<RecyclerAliasKey, OnChainAliasState?>> {
        if (keys.isEmpty()) return Result.success(emptyMap())

        val instanceIdKey = instanceId.toLong().toBigInteger()
        val storageKeys = keys.map { StorageKey4(instanceIdKey, it.valueExponent, it.recyclerIndex, it.alias.value) }

        return remoteStorageSource.queryCatching(chainId, at = at) {
            metadata.coinage.recyclerAliasStates.entries(storageKeys)
        }.map { states ->
            states.mapKeys { (key, _) -> RecyclerAliasKey(key.second, key.third, key.fourth.toDataByteArray()) }
                .ensureKeysWithNullDefault(keys)
        }
    }
}
