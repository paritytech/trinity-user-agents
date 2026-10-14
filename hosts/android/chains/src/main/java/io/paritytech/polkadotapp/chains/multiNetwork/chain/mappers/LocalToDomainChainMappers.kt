package io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers

import com.google.gson.Gson
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.AssetsAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.ConnectionState
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.ExternalApi
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Nodes.NodeSelectionStrategy
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.asGsonParsedNumber
import io.paritytech.polkadotapp.common.utils.fromJson
import io.paritytech.polkadotapp.common.utils.nullIfEmpty
import io.paritytech.polkadotapp.common.utils.parseArbitraryObject
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal.ApiType
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.ConnectionStateLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.NodeSelectionStrategyLocal
import io.paritytech.polkadotapp.database.model.chain.JoinedChainInfo
import timber.log.Timber
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type.Orml.SubType as OrmlSubType

private const val ASSET_NATIVE = "native"

// Keeping old value for the sake of compatibility with other configs
private const val ASSET_ASSETS = "statemine"
private const val ASSETS_EXTRAS_ID = "assetId"
private const val ASSETS_EXTRAS_PALLET_NAME = "palletName"
private const val ASSETS_IS_SUFFICIENT = "isSufficient"
private const val ASSETS_IS_SUFFICIENT_DEFAULT = false

private const val ASSET_ORML = "orml"
private const val ASSET_ORML_HYDRATION_EVM = "orml-hydration-evm"
private const val ORML_EXTRAS_CURRENCY_ID_SCALE = "currencyIdScale"
private const val ORML_EXTRAS_CURRENCY_TYPE = "currencyIdType"
private const val ORML_EXTRAS_EXISTENTIAL_DEPOSIT = "existentialDeposit"

private inline fun unsupportedOnError(creator: () -> Asset.Type): Asset.Type {
    return runCatching(creator)
        .onFailure { Timber.e(it, "Failed to construct chain type") }
        .getOrDefault(Asset.Type.Unsupported)
}

private fun mapChainAssetTypeFromRaw(
    type: String?,
    typeExtras: Map<String, Any?>?,
): Asset.Type = unsupportedOnError {
    when (type) {
        null, ASSET_NATIVE -> Asset.Type.Native

        ASSET_ASSETS -> {
            val idRaw = typeExtras?.get(ASSETS_EXTRAS_ID)!!
            val id = mapStatemineAssetIdFromRaw(idRaw)
            val palletName = typeExtras[ASSETS_EXTRAS_PALLET_NAME] as String?
            val isSufficient = typeExtras[ASSETS_IS_SUFFICIENT] as Boolean? ?: ASSETS_IS_SUFFICIENT_DEFAULT

            Asset.Type.Assets(id, palletName, isSufficient)
        }

        ASSET_ORML, ASSET_ORML_HYDRATION_EVM -> {
            Asset.Type.Orml(
                currencyIdScale = typeExtras!![ORML_EXTRAS_CURRENCY_ID_SCALE] as String,
                currencyIdType = typeExtras[ORML_EXTRAS_CURRENCY_TYPE] as String,
                existentialDeposit = (typeExtras[ORML_EXTRAS_EXISTENTIAL_DEPOSIT] as String).toBigInteger(),
                subType = determineOrmlSubtype(type)
            )
        }

        else -> Asset.Type.Unsupported
    }
}

private fun determineOrmlSubtype(type: String): OrmlSubType {
    return when (type) {
        ASSET_ORML -> OrmlSubType.DEFAULT
        ASSET_ORML_HYDRATION_EVM -> OrmlSubType.HYDRATION_EVM
        else -> error("Unknown orml token subtype: $type")
    }
}

private fun mapStatemineAssetIdFromRaw(rawValue: Any): AssetsAssetId {
    val asString = rawValue as? String ?: error("Invalid format")

    return if (asString.startsWith("0x")) {
        AssetsAssetId.ScaleEncoded(asString)
    } else {
        AssetsAssetId.Number(asString.asGsonParsedNumber())
    }
}

private fun mapExternalApisLocalToDomain(
    externalApiLocals: List<ChainExternalApiLocal>,
): List<ExternalApi> {
    return externalApiLocals
        .groupBy { it.apiType }
        .mapNotNull { (apiType, rows) ->
            when (apiType) {
                ApiType.HOP -> ExternalApi.Hop(urls = rows.map { it.url })
                ApiType.UNKNOWN -> null
            }
        }
}

private fun mapNodeSelectionFromLocal(local: NodeSelectionStrategyLocal): NodeSelectionStrategy {
    return when (local) {
        NodeSelectionStrategyLocal.ROUND_ROBIN -> NodeSelectionStrategy.ROUND_ROBIN
        NodeSelectionStrategyLocal.UNIFORM -> NodeSelectionStrategy.UNIFORM
        NodeSelectionStrategyLocal.UNKNOWN -> NodeSelectionStrategy.ROUND_ROBIN
    }
}

fun mapChainLocalToChain(
    chainLocal: JoinedChainInfo,
    gson: Gson,
): Chain {
    val nodes =
        chainLocal.getSortedNodes().map {
            Chain.Node(
                unformattedUrl = it.url,
                name = it.name,
                chainId = it.chainId,
                orderId = it.orderId
            )
        }
    val nodesConfig =
        Chain.Nodes(
            nodeSelectionStrategy = mapNodeSelectionFromLocal(chainLocal.chain.nodeSelectionStrategy),
            nodes = nodes
        )

    val assets = chainLocal.assets.map { mapChainAssetLocalToAsset(it, gson) }

    val explorers = chainLocal.explorers.map {
        Chain.Explorer(
            name = it.name,
            account = it.account,
            extrinsic = it.extrinsic,
            event = it.event,
            chainId = it.chainId
        )
    }

    val types = chainLocal.chain.types?.let {
        Chain.Types(
            url = it.url.nullIfEmpty(),
            overridesCommon = it.overridesCommon
        )
    }

    val externalApis = mapExternalApisLocalToDomain(chainLocal.externalApis)

    val additional =
        chainLocal.chain.additional?.let { raw ->
            gson.fromJson<Chain.Additional>(raw)
        }

    return with(chainLocal.chain) {
        Chain(
            id = id,
            genesisHash = genesisHash.fromHex().toDataByteArray(),
            parentId = parentId,
            name = name,
            assets = assets,
            types = types,
            nodes = nodesConfig,
            explorers = explorers,
            externalApis = externalApis,
            addressPrefix = prefix,
            isEthereumBased = isEthereumBased,
            isTestNet = isTestNet,
            hasSubstrateRuntime = hasSubstrateRuntime,
            connectionState = mapConnectionStateFromLocal(connectionState),
            additional = additional
        )
    }
}

fun mapChainAssetLocalToAsset(
    local: ChainAssetLocal,
    gson: Gson,
): Asset {
    val typeExtrasParsed = local.typeExtras?.let(gson::parseArbitraryObject)

    return Asset(
        id = local.id,
        symbol = local.symbol,
        precision = local.precision,
        name = local.name,
        chainId = local.chainId,
        priceId = local.priceId,
        type = mapChainAssetTypeFromRaw(local.type, typeExtrasParsed),
        enabled = local.enabled
    )
}

fun mapConnectionStateToLocal(domain: ConnectionState): ConnectionStateLocal {
    return when (domain) {
        ConnectionState.FULL_SYNC -> ConnectionStateLocal.FULL_SYNC
        ConnectionState.LIGHT_SYNC -> ConnectionStateLocal.LIGHT_SYNC
        ConnectionState.DISABLED -> ConnectionStateLocal.DISABLED
    }
}

private fun mapConnectionStateFromLocal(local: ConnectionStateLocal): ConnectionState {
    return when (local) {
        ConnectionStateLocal.FULL_SYNC -> ConnectionState.FULL_SYNC
        ConnectionStateLocal.LIGHT_SYNC -> ConnectionState.LIGHT_SYNC
        ConnectionStateLocal.DISABLED -> ConnectionState.DISABLED
    }
}
