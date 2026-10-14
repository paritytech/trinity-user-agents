package io.paritytech.polkadotapp.feature_coinage_impl.data.repository

import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.storage.source.subscribeCatching
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.data.memory.getCatching
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.database.dao.CoinDao
import io.paritytech.polkadotapp.database.dao.CoinHopsUpdateLocal
import io.paritytech.polkadotapp.database.dao.CoinUpdateLocal
import io.paritytech.polkadotapp.database.model.CoinLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinUpdate
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinsByOwner
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.getMaximumAge
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.maxConsolidation
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.queryPerInstallation
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.toCoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.data.mappers.decodeCoinHops
import io.paritytech.polkadotapp.feature_coinage_impl.data.mappers.encodeCoinHops
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.getNextIndex
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

private const val RECYCLING_AGE_OFFSET = 2

interface CoinRepository {
    suspend fun save(coin: Coin)

    suspend fun saveAll(coins: List<Coin>)

    suspend fun saveNew(coin: Coin)

    suspend fun saveNew(coins: List<Coin>)

    /** Every coin we know of, on chain or not. */
    fun subscribeAllCoins(): Flow<List<Coin>>

    fun subscribeAllCoinsWithUnknownAge(): Flow<List<Coin>>

    suspend fun getAllCoins(): List<Coin>

    /** Only the coins at [accountIds], for a caller watching a few of them rather than the wallet. */
    fun subscribeCoinsBy(accountIds: List<AccountId>): Flow<List<Coin>>

    suspend fun getCoinsBy(derivationIndices: List<CoinageKeyIndex>): List<Coin>

    suspend fun getCoinRecyclingAge(): Result<Int>

    suspend fun getNextDerivationIndex(installation: CoinageInstallationId): Int

    suspend fun subscribeCoinsInfoFor(chainId: ChainId, accounts: List<AccountId>): Flow<Result<Map<AccountId, OnChainCoinInfo?>>>

    suspend fun fetchCoinsInfoFor(chainId: ChainId, accounts: List<AccountId>): Result<Map<AccountId, OnChainCoinInfo?>>

    suspend fun updateCoins(updates: List<CoinUpdate>)

    /** Fills in the hops of coins that arrived without any, once the chain has reported their age. */
    suspend fun updateCoinHops(updates: Map<CoinageKeyIndex, List<Hop>>)

    /** Coins the chain currently holds. Says nothing about whether they may be spent — see the ledger. */
    suspend fun getOnChainCoins(): List<Coin>

    suspend fun getOnChainCoinsWithAgeAtLeast(minAge: Int): List<Coin>

    fun subscribeOnChainCoins(): Flow<List<Coin>>

    suspend fun fetchMaxConsolidation(chainId: ChainId): Result<Int>
}

class RealCoinRepository @Inject constructor(
    private val coinDao: CoinDao,
    private val chainRegistry: ChainRegistry,
    @param:RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : CoinRepository {
    // The runtime exposes the maximum age as a view function, so it is read once per session rather than
    // per verdict. Unwrapping inside the compute keeps a failed read out of the cache; getCatching re-wraps.
    private val maximumAgeCache = SingleValueCache {
        viewFunctionsApi.forChain(chainAssetProvider.chainId()).getMaximumAge().getOrThrow().toInt()
    }

    override suspend fun save(coin: Coin) {
        coinDao.insert(coin.toLocal())
    }

    override suspend fun saveAll(coins: List<Coin>) {
        coinDao.insertAll(coins.map { it.toLocal() })
    }

    override suspend fun saveNew(coin: Coin) {
        coinDao.insertNew(listOf(coin.toLocal()))
    }

    override suspend fun saveNew(coins: List<Coin>) {
        coinDao.insertNew(coins.map { it.toLocal() })
    }

    override fun subscribeAllCoins(): Flow<List<Coin>> {
        return coinDao.subscribeAll().mapList { it.toDomain() }
    }

    override suspend fun getCoinRecyclingAge(): Result<Int> {
        return maximumAgeCache.getCatching().map { it - RECYCLING_AGE_OFFSET }
    }

    override fun subscribeAllCoinsWithUnknownAge(): Flow<List<Coin>> {
        return coinDao.subscribeAllCoinsWithUnknownAge().mapList { it.toDomain() }
    }

    override suspend fun getNextDerivationIndex(installation: CoinageInstallationId): Int {
        return coinDao.getMaxDerivationIndex(installation.value.value).getNextIndex()
    }

    override suspend fun fetchCoinsInfoFor(chainId: ChainId, accounts: List<AccountId>): Result<Map<AccountId, OnChainCoinInfo?>> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.coinage.coinsByOwner.entries(accounts)
        }
    }

    override suspend fun subscribeCoinsInfoFor(chainId: ChainId, accounts: List<AccountId>): Flow<Result<Map<AccountId, OnChainCoinInfo?>>> {
        return remoteStorageSource.subscribeCatching(chainId) {
            metadata.coinage.coinsByOwner.observe(accounts)
        }
    }

    override suspend fun fetchMaxConsolidation(chainId: ChainId): Result<Int> {
        return runCatching {
            chainRegistry.withRuntime(chainId) {
                runtime.metadata.coinage.maxConsolidation
            }
        }
    }

    override suspend fun updateCoins(updates: List<CoinUpdate>) {
        val updateLocals = updates.map { it.toLocal() }
        coinDao.updateCoins(updateLocals)
    }

    override suspend fun updateCoinHops(updates: Map<CoinageKeyIndex, List<Hop>>) {
        if (updates.isEmpty()) return

        coinDao.updateCoinHops(
            updates.map { (keyIndex, hops) ->
                CoinHopsUpdateLocal(
                    installationId = keyIndex.installation.value.value,
                    derivationIndex = keyIndex.item,
                    hops = hops.encodeCoinHops()
                )
            }
        )
    }

    override suspend fun getOnChainCoins(): List<Coin> {
        return coinDao.getOnChainCoins().map { it.toDomain() }
    }

    override suspend fun getOnChainCoinsWithAgeAtLeast(minAge: Int): List<Coin> {
        return coinDao.getCoinsWithKnownAgeAtLeast(minAge).map { it.toDomain() }
    }

    override fun subscribeOnChainCoins(): Flow<List<Coin>> {
        return coinDao.subscribeOnChainCoins().mapList { it.toDomain() }
    }

    override suspend fun getAllCoins(): List<Coin> {
        return coinDao.getAll().map { it.toDomain() }
    }

    override fun subscribeCoinsBy(accountIds: List<AccountId>): Flow<List<Coin>> {
        if (accountIds.isEmpty()) return flowOf(emptyList())

        return coinDao.subscribeBy(accountIds.map { it.value }).mapList { it.toDomain() }
    }

    override suspend fun getCoinsBy(derivationIndices: List<CoinageKeyIndex>): List<Coin> {
        return derivationIndices.queryPerInstallation { installationId, items ->
            coinDao.getByDerivationIndices(installationId, items)
        }.map { it.toDomain() }
    }

    fun CoinLocal.toDomain(): Coin {
        return Coin(
            derivationIndex = installationId.toCoinageKeyIndex(derivationIndex),
            valueExponent = ValueExponent(valueExponent),
            age = ageValue?.let(Coin.Age::Known) ?: Coin.Age.Unknown,
            isOnChain = onChain,
            accountId = accountId.intoAccountId(),
            provenance = CoinProvenance(
                recyclerFungibility = recyclerFungibility?.let(RecyclerFungibility::ofPercent),
                // A corrupt blob costs one details row its circles; failing here would take down the balance
                // stream this mapper sits inside, so the row is drawn as though the coin had no history.
                hops = hops.decodeCoinHops()
                    .logFailure("Can't decode hops of coin $derivationIndex")
                    .getOrDefault(emptyList()),
                incomingBundleSize = incomingBundleSize,
            )
        )
    }

    fun Coin.toLocal(): CoinLocal {
        return CoinLocal(
            installationId = derivationIndex.installation.value.value,
            derivationIndex = derivationIndex.item,
            accountId = accountId.value,
            valueExponent = valueExponent.value,
            ageValue = (age as? Coin.Age.Known)?.value,
            onChain = isOnChain,
            recyclerFungibility = provenance.recyclerFungibility?.percent,
            // Null rather than an encoded empty list, so a coin with no hops costs no blob.
            hops = provenance.hops.takeIf { it.isNotEmpty() }?.encodeCoinHops(),
            incomingBundleSize = provenance.incomingBundleSize,
        )
    }

    private fun CoinUpdate.toLocal() = CoinUpdateLocal(accountId = accountId, onChain = onChain, age = age)
}
