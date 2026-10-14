package io.paritytech.polkadotapp.feature_pgas_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.constant
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.numberConstant
import io.paritytech.polkadotapp.chains.util.pgas
import io.paritytech.polkadotapp.common.utils.scale.BigEndianU32Scale
import java.math.BigInteger

@JvmInline
value class PgasApi(override val module: Module) : QueryableModule

val RuntimeMetadata.pgas: PgasApi
    get() = PgasApi(pgas())

context(withRuntime: WithRuntime)
val PgasApi.maxClaimsPerPeriodPerPerson: UInt
    get() = constant("MaxClaimsPerPeriodPerPerson")

context(withRuntime: WithRuntime)
val PgasApi.maxClaimsPerPeriodPerLitePerson: UInt
    get() = constant("MaxClaimsPerPeriodPerLitePerson")

context(withRuntime: WithRuntime)
val PgasApi.pgasClaimAmount: BigInteger
    get() = module.numberConstant("PgasClaimAmount")

context(withRuntime: WithRuntime)
val PgasApi.claimedGasAliases: QueryableStorageEntry2<BigEndianU32Scale, BandersnatchAlias, Unit>
    get() = storage2("ClaimedGasAliases")
