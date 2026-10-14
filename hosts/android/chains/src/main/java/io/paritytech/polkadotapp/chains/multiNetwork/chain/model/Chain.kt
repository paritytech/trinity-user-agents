package io.paritytech.polkadotapp.chains.multiNetwork.chain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.Identifiable
import java.math.BigInteger

typealias ChainId = String
typealias ChainAssetId = Int
typealias GenesisHash = DataByteArray
typealias StringTemplate = String
typealias TokenSymbol = String

typealias ExplorerTemplateExtractor = (Chain.Explorer) -> StringTemplate?

data class FullChainAssetId(val chainId: ChainId, val assetId: ChainAssetId)

data class Chain(
    val id: ChainId,
    val genesisHash: GenesisHash,
    val name: String,
    val assets: List<Asset>,
    val nodes: Nodes,
    val explorers: List<Explorer>,
    val externalApis: List<ExternalApi>,
    val addressPrefix: Int,
    val types: Types?,
    val isEthereumBased: Boolean,
    val isTestNet: Boolean,
    val hasSubstrateRuntime: Boolean,
    val connectionState: ConnectionState,
    val parentId: ChainId?,
    val additional: Additional?,
) : Identifiable {
    companion object // extensions

    val assetsById = assets.associateBy(Asset::id)

    data class Additional(
        val defaultBlockTimeMillis: Long?,
        val finalityGapIdeal: Int?,
        val finalityGapOutage: Int?,
    )

    data class Types(
        val url: String?,
        val overridesCommon: Boolean,
    )

    data class Asset(
        val id: ChainAssetId,
        val priceId: String?,
        val chainId: ChainId,
        val symbol: TokenSymbol,
        val precision: Int,
        val type: Type,
        val name: String,
        val enabled: Boolean,
    ) : Identifiable {
        sealed class Type {
            data object Native : Type()

            data class Assets(
                val id: AssetsAssetId,
                val palletName: String?,
                val isSufficient: Boolean,
            ) : Type()

            data class Orml(
                val currencyIdScale: String,
                val currencyIdType: String,
                val existentialDeposit: BigInteger,
                val subType: SubType,
            ) : Type() {
                enum class SubType {
                    DEFAULT, HYDRATION_EVM
                }
            }

            data object Unsupported : Type()
        }

        override val identifier = "$chainId:$id"
    }

    data class Nodes(
        val nodeSelectionStrategy: NodeSelectionStrategy,
        val nodes: List<Node>,
    ) {
        enum class NodeSelectionStrategy {
            ROUND_ROBIN,
            UNIFORM,
        }
    }

    data class Node(
        val chainId: ChainId,
        val unformattedUrl: String,
        val name: String,
        val orderId: Int,
    ) : Identifiable {
        enum class ConnectionType {
            HTTP,
            WS,
            UNKNOWN,
        }

        val connectionType = when {
            unformattedUrl.startsWith("wss://") -> ConnectionType.WS
            unformattedUrl.startsWith("ws://") -> ConnectionType.WS
            unformattedUrl.startsWith("https://") -> ConnectionType.HTTP
            else -> ConnectionType.UNKNOWN
        }

        override val identifier: String = "$chainId:$unformattedUrl"
    }

    data class Explorer(
        val chainId: ChainId,
        val name: String,
        val account: StringTemplate?,
        val extrinsic: StringTemplate?,
        val event: StringTemplate?,
    ) : Identifiable {
        override val identifier = "$chainId:$name"
    }

    sealed interface ExternalApi {
        data class Hop(val urls: List<String>) : ExternalApi
    }

    enum class ConnectionState {
        /**
         * Runtime sync is performed for the chain and the chain can be considered ready for any operation
         */
        FULL_SYNC,

        /**
         * Websocket connection is established for the chain, but runtime is not synced.
         * Thus, only runtime-independent operations can be performed
         */
        LIGHT_SYNC,

        /**
         * Chain is completely disabled - it does not initialize websockets not allocates any other resources
         */
        DISABLED,
    }

    override val identifier: String = id
}

enum class TypesUsage {
    BASE,
    OWN,
    BOTH,
    NONE,
}
