package io.paritytech.polkadotapp.feature_xcm_api.multiLocation

import io.novasama.substrate_sdk_android.extensions.tryFindNonNull
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.AddressScheme
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.ParaId
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.Junction
import java.math.BigInteger

abstract class MultiLocation(
    open val interior: Interior
) {
    sealed class Interior {
        data object Here : Interior()

        class Junctions(junctions: List<Junction>) : Interior() {
            val junctions = junctions.sorted()

            override fun equals(other: Any?): Boolean {
                if (other !is Junctions) return false
                return junctions == other.junctions
            }

            override fun hashCode(): Int {
                return junctions.hashCode()
            }

            override fun toString(): String {
                return junctions.toString()
            }
        }
    }

    sealed class Junction {
        data class ParachainId(val id: ParaId) : Junction() {
            constructor(id: Int) : this(id.toBigInteger())
        }

        data class GeneralKey(val key: DataByteArray) : Junction()

        data class PalletInstance(val index: BigInteger) : Junction()

        data class GeneralIndex(val index: BigInteger) : Junction()

        data class AccountKey20(val accountId: AccountId) : Junction()

        data class AccountId32(val accountId: AccountId) : Junction()

        data class GlobalConsensus(val networkId: NetworkId) : Junction() {
            constructor(genesisHash: GenesisHash) : this(NetworkId.Substrate(genesisHash))
        }

        object Unsupported : Junction()
    }

    sealed class NetworkId {
        data class Substrate(val genesisHash: GenesisHash) : NetworkId()

        data class Ethereum(val chainId: Int) : NetworkId()
    }
}

val Junction.order
    get() = when (this) {
        is Junction.GlobalConsensus -> 0

        is Junction.ParachainId -> 1

        // All of these are on the same "level" - mutually exhaustive
        is Junction.PalletInstance,
        is Junction.AccountKey20,
        is Junction.AccountId32 -> 2

        is Junction.GeneralKey,
        is Junction.GeneralIndex -> 3

        Junction.Unsupported -> Int.MAX_VALUE
    }

val MultiLocation.junctions: List<Junction>
    get() = when (val interior = interior) {
        MultiLocation.Interior.Here -> emptyList()
        is MultiLocation.Interior.Junctions -> interior.junctions
    }

fun List<Junction>.toInterior() = when (size) {
    0 -> MultiLocation.Interior.Here
    else -> MultiLocation.Interior.Junctions(this)
}

fun Junction.toInterior() = MultiLocation.Interior.Junctions(listOf(this))

fun MultiLocation.Interior.isHere() = this is MultiLocation.Interior.Here

fun MultiLocation.accountId(): AccountId? {
    return junctions.tryFindNonNull {
        when (it) {
            is Junction.AccountId32 -> it.accountId
            is Junction.AccountKey20 -> it.accountId
            else -> null
        }
    }
}

fun MultiLocation.Interior.asLocation(): AbsoluteMultiLocation {
    return AbsoluteMultiLocation(this)
}

fun List<Junction>.asLocation(): AbsoluteMultiLocation {
    return toInterior().asLocation()
}

fun Junction.asLocation(): AbsoluteMultiLocation {
    return toInterior().asLocation()
}

fun AccountId.toMultiLocation() = RelativeMultiLocation(
    parents = 0,
    interior = Junctions(
        when (AddressScheme.findFromAccountId(this)) {
            AddressScheme.SUBSTRATE -> Junction.AccountId32(this)
            AddressScheme.EVM -> Junction.AccountKey20(this)
            else -> throw IllegalArgumentException("Unsupported account id length: ${value.size}")
        }
    )
)

fun Junctions(vararg junctions: Junction) = MultiLocation.Interior.Junctions(junctions.toList())

fun MultiLocation.paraIdOrNull(): ParaId? {
    return junctions.filterIsInstance<Junction.ParachainId>()
        .firstOrNull()
        ?.id
}

private fun List<Junction>.sorted(): List<Junction> {
    return sortedBy(Junction::order)
}
