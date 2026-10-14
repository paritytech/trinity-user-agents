package io.paritytech.polkadotapp.feature_coinage_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.StorageKey4
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.storage.source.subscribeCatching
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.ensureKeysWithDefault
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.common.utils.mapValuesNotNull
import io.paritytech.polkadotapp.database.dao.RecyclerVoucherDao
import io.paritytech.polkadotapp.database.dao.RecyclerVoucherLocationUpdate
import io.paritytech.polkadotapp.database.model.RecyclerVoucherLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.filterInRecycler
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.RecyclerStorageKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.recyclerAliasStates
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.recyclersCoinToRecycler
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.recyclersUnloadedCount
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.queryPerInstallation
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.toCoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.getNextIndex
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import java.math.BigInteger
import javax.inject.Inject
import kotlin.time.Instant

interface VoucherRepository {
    suspend fun save(voucher: RecyclerVoucher)

    fun subscribeAllVouchers(): Flow<List<RecyclerVoucher>>

    suspend fun updateRecyclerState(updates: Map<BandersnatchPublicKey, VoucherRecyclerUpdate>)

    suspend fun getNextDerivationIndex(installation: CoinageInstallationId): Int

    suspend fun saveAll(vouchers: List<RecyclerVoucher>)

    suspend fun saveNew(voucher: RecyclerVoucher)

    suspend fun saveNew(vouchers: List<RecyclerVoucher>)

    /** Vouchers that are in a recycler. Says nothing about whether they may be used — see the ledger. */
    suspend fun getVouchersInRecycler(): List<RecyclerVoucher>

    suspend fun getAllVouchers(): List<RecyclerVoucher>

    suspend fun fetchValuesForKeys(
        chainId: ChainId,
        instanceId: CoinageInstanceId,
        voucherKeys: List<BandersnatchPublicKey>
    ): Result<Map<BandersnatchPublicKey, ValueExponent>>

    suspend fun getByRingVrfKeyIndices(indices: List<CoinageKeyIndex>): List<RecyclerVoucher>

    fun subscribeVouchersInRecycler(): Flow<List<RecyclerVoucher>>

    suspend fun fetchRecyclerAliasStates(
        chainId: ChainId,
        keys: List<StorageKey4<BigInteger, BigInteger, BigInteger, ByteArray>>
    ): Result<Map<String, OnChainAliasState?>>

    /**
     * How many keys have been unloaded from each of [keys], zero for a recycler the runtime holds no entry
     * for. Every requested key is present in the emitted map.
     */
    fun subscribeUnloadedCounts(
        chainId: ChainId,
        instanceId: CoinageInstanceId,
        keys: List<RecyclerKey>
    ): Flow<Result<Map<RecyclerKey, Int>>>
}

/** What one tick of the location service resolved about the ring a voucher sits in. */
data class VoucherRecyclerUpdate(
    val location: RecyclerVoucher.Location.InRecycler,
    /** Null when the ring capacity could not be read, which leaves the stored fungibilities standing. */
    val recyclerFungibility: RecyclerFungibility?,
    val maxRecyclerFungibility: RecyclerFungibility?,
)

fun VoucherRepository.subscribeReadyToUseVouchers(): Flow<List<RecyclerVoucher>> {
    return subscribeVouchersInRecycler().map { vouchers -> vouchers.filterInRecycler() }
}

class RealVoucherRepository @Inject constructor(
    private val recyclerVoucherDao: RecyclerVoucherDao,
    @param:RemoteSourceQualifier private val remoteStorageSource: StorageDataSource
) : VoucherRepository {
    override suspend fun save(voucher: RecyclerVoucher) {
        recyclerVoucherDao.insert(voucher.toLocal())
    }

    override suspend fun saveAll(vouchers: List<RecyclerVoucher>) {
        recyclerVoucherDao.insertAll(vouchers.map { it.toLocal() })
    }

    override suspend fun saveNew(voucher: RecyclerVoucher) {
        recyclerVoucherDao.insertNew(listOf(voucher.toLocal()))
    }

    override suspend fun saveNew(vouchers: List<RecyclerVoucher>) {
        recyclerVoucherDao.insertNew(vouchers.map { it.toLocal() })
    }

    override fun subscribeAllVouchers(): Flow<List<RecyclerVoucher>> {
        return recyclerVoucherDao.subscribeAll().mapList { it.toDomain() }
    }

    override suspend fun updateRecyclerState(updates: Map<BandersnatchPublicKey, VoucherRecyclerUpdate>) {
        val rows = updates.map { (publicKey, update) ->
            RecyclerVoucherLocationUpdate(
                ringVrfPublicKey = publicKey.value,
                recyclerIndex = update.location.recyclerIndex.value.toInt(),
                recyclerMembers = update.location.recyclerMembers,
                enteredAt = update.location.enteredAt?.toEpochMilliseconds(),
                recyclerFungibility = update.recyclerFungibility?.percent,
                maxRecyclerFungibility = update.maxRecyclerFungibility?.percent
            )
        }
        recyclerVoucherDao.updateLocations(rows)
    }

    override fun subscribeUnloadedCounts(
        chainId: ChainId,
        instanceId: CoinageInstanceId,
        keys: List<RecyclerKey>
    ): Flow<Result<Map<RecyclerKey, Int>>> {
        if (keys.isEmpty()) return flowOf(Result.success(emptyMap()))

        val instanceIdKey = instanceId.toLong().toBigInteger()
        val byStorageKey = keys.associateBy { key ->
            RecyclerStorageKey(
                instanceId = instanceIdKey,
                denomination = key.exponent.value.toBigInteger(),
                ringIndex = key.recyclerIndex.value
            )
        }

        return remoteStorageSource.subscribeCatching(chainId) {
            metadata.coinage.recyclersUnloadedCount.observe(byStorageKey.keys.toList())
        }.map { result ->
            result.map { counts ->
                // Absent means nothing has been unloaded, so every requested key still gets an answer.
                counts.mapValuesNotNull { (_, count) -> count?.toInt() }
                    .ensureKeysWithDefault(byStorageKey.keys, default = NOTHING_UNLOADED)
                    .mapKeys { (storageKey, _) -> byStorageKey.getValue(storageKey) }
            }
        }
    }

    override suspend fun getByRingVrfKeyIndices(indices: List<CoinageKeyIndex>): List<RecyclerVoucher> {
        return indices.queryPerInstallation { installationId, items ->
            recyclerVoucherDao.getByRingVrfKeyIndices(installationId, items)
        }.map { it.toDomain() }
    }

    override suspend fun getNextDerivationIndex(installation: CoinageInstallationId): Int {
        return recyclerVoucherDao.getMaxRingVrfKeyIndex(installation.value.value).getNextIndex()
    }

    override suspend fun fetchRecyclerAliasStates(
        chainId: ChainId,
        keys: List<StorageKey4<BigInteger, BigInteger, BigInteger, ByteArray>>
    ): Result<Map<String, OnChainAliasState?>> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.coinage.recyclerAliasStates.entries(keys)
        }
            .map {
                it.mapKeys { (key, _) -> key.fourth.toDataByteArray().toString() }
            }
    }

    override suspend fun getVouchersInRecycler(): List<RecyclerVoucher> {
        return recyclerVoucherDao.getVouchersInRecycler().map { it.toDomain() }
    }

    override suspend fun getAllVouchers(): List<RecyclerVoucher> {
        return recyclerVoucherDao.getAllVouchers().map { it.toDomain() }
    }

    override fun subscribeVouchersInRecycler(): Flow<List<RecyclerVoucher>> {
        return recyclerVoucherDao.subscribeVouchersInRecycler().mapList { it.toDomain() }
    }

    override suspend fun fetchValuesForKeys(
        chainId: ChainId,
        instanceId: CoinageInstanceId,
        voucherKeys: List<BandersnatchPublicKey>
    ): Result<Map<BandersnatchPublicKey, ValueExponent>> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.coinage.recyclersCoinToRecycler.entries(voucherKeys)
        }
            .map { entries ->
                entries
                    .filterValues { location -> location.instanceId.toUInt() == instanceId }
                    .mapValues { (_, location) -> ValueExponent(location.value) }
            }
    }

    private fun RecyclerVoucherLocal.toDomain(): RecyclerVoucher {
        return RecyclerVoucher(
            ringVrfKeyIndex = installationId.toCoinageKeyIndex(ringVrfKeyIndex),
            ringVrfPublicKey = ringVrfPublicKey.toDataByteArray(),
            recyclerValue = ValueExponent(recyclerValue),
            location = toDomainLocation(),
            recyclerFungibility = RecyclerFungibility.ofPercent(recyclerFungibility),
            maxRecyclerFungibility = maxRecyclerFungibility?.let(RecyclerFungibility::ofPercent),
        )
    }

    private fun RecyclerVoucherLocal.toDomainLocation(): RecyclerVoucher.Location {
        val index = locationRecyclerIndex ?: return RecyclerVoucher.Location.Unknown

        // Written together by the location service, so one without the other is a corrupt row rather than a
        // state worth guessing at.
        val members = requireNotNull(recyclerMembers) { "Voucher in recycler $index has no member count" }

        return RecyclerVoucher.Location.InRecycler(
            recyclerIndex = RecyclerIndex(index.toBigInteger()),
            recyclerMembers = members,
            enteredAt = enteredAt?.let(Instant::fromEpochMilliseconds)
        )
    }

    private fun RecyclerVoucher.toLocal(): RecyclerVoucherLocal {
        val inRecycler = location as? RecyclerVoucher.Location.InRecycler
        return RecyclerVoucherLocal(
            installationId = ringVrfKeyIndex.installation.value.value,
            ringVrfKeyIndex = ringVrfKeyIndex.item,
            ringVrfPublicKey = ringVrfPublicKey.value,
            recyclerValue = recyclerValue.value,
            locationRecyclerIndex = inRecycler?.recyclerIndex?.value?.toInt(),
            recyclerMembers = inRecycler?.recyclerMembers,
            enteredAt = inRecycler?.enteredAt?.toEpochMilliseconds(),
            recyclerFungibility = recyclerFungibility.percent,
            maxRecyclerFungibility = maxRecyclerFungibility?.percent,
        )
    }
}

/** A recycler the runtime holds no counter for has had nothing unloaded from it. */
private const val NOTHING_UNLOADED = 0
