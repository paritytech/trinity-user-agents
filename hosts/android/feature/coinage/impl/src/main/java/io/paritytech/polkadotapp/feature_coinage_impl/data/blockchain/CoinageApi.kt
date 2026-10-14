package io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry4
import io.paritytech.polkadotapp.chains.storage.source.query.api.constant
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage4
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.coinage
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainInstanceRecord
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainRecyclerLocation
import kotlinx.serialization.Serializable
import java.math.BigInteger

typealias TokenPeriod = BigIntegerSerializable
typealias CounterAlias = DataByteArray

@JvmInline
value class CoinageApi(override val module: Module) : QueryableModule

val RuntimeMetadata.coinage: CoinageApi
    get() = CoinageApi(coinage())

context(withRuntime: WithRuntime)
val CoinageApi.minExponent: Int
    get() = constant("MinimumExponent")

context(withRuntime: WithRuntime)
val CoinageApi.maxExponent: Int
    get() = constant("MaximumExponent")

context(withRuntime: WithRuntime)
val CoinageApi.consumedFreeUnloadTokens: QueryableStorageEntry2<TokenPeriod, CounterAlias, Unit>
    get() = storage2("ConsumedFreeUnloadTokens")

context(withRuntime: WithRuntime)
val CoinageApi.coinsByOwner: QueryableStorageEntry1<AccountId, OnChainCoinInfo>
    get() = storage1("CoinsByOwner")

context(withRuntime: WithRuntime)
val CoinageApi.instances: QueryableStorageEntry1<BigInteger, OnChainInstanceRecord>
    get() = storage1("Instances")

context(withRuntime: WithRuntime)
val CoinageApi.unloadTokenTimePeriodPeopleLitePeople: Long
    get() = constant("UnloadTokenTimePeriodPeopleLitePeople")

context(withRuntime: WithRuntime)
val CoinageApi.recyclersCoinToRecycler: QueryableStorageEntry1<BandersnatchPublicKey, OnChainRecyclerLocation>
    get() = storage1("RecyclersCoinToRecycler")

val CoinageApi.recyclerAliasStates: QueryableStorageEntry4<BigInteger, BigInteger, BigInteger, ByteArray, OnChainAliasState>
    get() = storage4("RecyclerAliasStates")

/**
 * How many of a recycler's keys have been unloaded. An absent entry means none, not "unknown" — the runtime
 * only writes one once something has actually been unloaded.
 *
 * Keyed by `(instance id, denomination, ring index)`, but as **one** `Twox64Concat` hash over the whole
 * tuple rather than three hashed components — unlike [recyclerAliasStates] next to it, which really is a
 * four-component map. That is why this is a `storage1` over [RecyclerStorageKey]: the three values travel
 * as a single key argument. Passing them as three would hash each on its own and address storage that does
 * not exist.
 */
context(withRuntime: WithRuntime)
val CoinageApi.recyclersUnloadedCount: QueryableStorageEntry1<RecyclerStorageKey, BigIntegerSerializable>
    get() = storage1("RecyclersUnloadedCount")

/**
 * One recycler, as the tuple the coinage pallet keys by.
 *
 * A data class because responses are matched back to the keys that were asked for, which needs structural
 * equality rather than identity.
 */
@Serializable
@AsTuple
data class RecyclerStorageKey(
    val instanceId: BigIntegerSerializable,
    val denomination: BigIntegerSerializable,
    val ringIndex: BigIntegerSerializable
)

context(withRuntime: WithRuntime)
val CoinageApi.maxConsolidation: Int
    get() = constant("MaxConsolidation")
