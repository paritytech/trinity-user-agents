package io.paritytech.polkadotapp.chains.multiNetwork.chain.mappers

import com.google.gson.Gson
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.model.ChainAssetRemote
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.model.ChainRemote
import io.paritytech.polkadotapp.common.utils.asGsonParsedIntOrNull
import io.paritytech.polkadotapp.common.utils.asGsonParsedLongOrNull
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExplorerLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal.ApiType
import io.paritytech.polkadotapp.database.model.chain.ChainLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.ConnectionStateLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal.NodeSelectionStrategyLocal
import io.paritytech.polkadotapp.database.model.chain.ChainNodeLocal

private const val ETHEREUM_OPTION = "ethereumBased"
private const val TESTNET_OPTION = "testnet"
private const val NO_SUBSTRATE_RUNTIME = "noSubstrateRuntime"
const val DEFAULT_BLOCK_TIME = "defaultBlockTime"
const val FINALITY_GAP_IDEAL = "finalityGapIdeal"
const val FINALITY_GAP_OUTAGE = "finalityGapOutage"

private const val EXTERNAL_API_HOP = "hop"

fun mapRemoteChainToLocal(
    chainRemote: ChainRemote,
    oldChain: ChainLocal?,
    gson: Gson,
): ChainLocal {
    val types = chainRemote.types?.let {
        ChainLocal.TypesConfig(
            url = it.url.orEmpty(),
            overridesCommon = it.overridesCommon
        )
    }

    val additional = chainRemote.additional?.let {
        Chain.Additional(
            defaultBlockTimeMillis = it[DEFAULT_BLOCK_TIME].asGsonParsedLongOrNull(),
            finalityGapIdeal = it[FINALITY_GAP_IDEAL].asGsonParsedIntOrNull(),
            finalityGapOutage = it[FINALITY_GAP_OUTAGE].asGsonParsedIntOrNull(),
        )
    }

    val chainLocal =
        with(chainRemote) {
            val optionsOrEmpty = options.orEmpty()

            ChainLocal(
                id = chainId,
                genesisHash = genesisHash ?: chainId,
                parentId = parentId,
                name = name,
                types = types,
                prefix = addressPrefix,
                isEthereumBased = ETHEREUM_OPTION in optionsOrEmpty,
                isTestNet = TESTNET_OPTION in optionsOrEmpty,
                hasSubstrateRuntime = NO_SUBSTRATE_RUNTIME !in optionsOrEmpty,
                connectionState = determineConnectionState(chainRemote, oldChain),
                additional = gson.toJson(additional),
                nodeSelectionStrategy = mapNodeSelectionStrategyToLocal(nodeSelectionStrategy)
            )
        }

    return chainLocal
}

private fun mapNodeSelectionStrategyToLocal(remote: String?): NodeSelectionStrategyLocal {
    return when (remote) {
        null, "roundRobin" -> NodeSelectionStrategyLocal.ROUND_ROBIN
        "uniform" -> NodeSelectionStrategyLocal.UNIFORM
        else -> NodeSelectionStrategyLocal.UNKNOWN
    }
}

private fun determineConnectionState(
    remoteChain: ChainRemote,
    oldLocalChain: ChainLocal?,
): ConnectionStateLocal {
    // full sync for everything for now
    return ConnectionStateLocal.FULL_SYNC
}

fun mapRemoteAssetToLocal(
    chainRemote: ChainRemote,
    assetRemote: ChainAssetRemote,
    gson: Gson,
    isEnabled: Boolean,
): ChainAssetLocal {
    return ChainAssetLocal(
        id = assetRemote.assetId,
        symbol = assetRemote.symbol,
        precision = assetRemote.precision,
        chainId = chainRemote.chainId,
        name = assetRemote.name ?: chainRemote.name,
        priceId = assetRemote.priceId,
        type = assetRemote.type,
        typeExtras = gson.toJson(assetRemote.typeExtras),
        enabled = isEnabled
    )
}

fun mapRemoteNodesToLocal(chainRemote: ChainRemote): List<ChainNodeLocal> {
    return chainRemote.nodes.mapIndexed { index, chainNodeRemote ->
        ChainNodeLocal(
            url = chainNodeRemote.url,
            name = chainNodeRemote.name,
            chainId = chainRemote.chainId,
            orderId = index
        )
    }
}

fun mapRemoteExplorersToLocal(chainRemote: ChainRemote): List<ChainExplorerLocal> {
    val explorers =
        chainRemote.explorers?.map {
            ChainExplorerLocal(
                chainId = chainRemote.chainId,
                name = it.name,
                extrinsic = it.extrinsic,
                account = it.account,
                event = it.event
            )
        }

    return explorers.orEmpty()
}

fun mapExternalApisToLocal(chainRemote: ChainRemote): List<ChainExternalApiLocal> {
    return chainRemote.externalApi?.flatMap { (apiType, urls) ->
        urls.map { url ->
            ChainExternalApiLocal(
                chainId = chainRemote.chainId,
                apiType = mapApiTypeRemoteToLocal(apiType),
                parameters = null,
                url = url
            )
        }
    }.orEmpty()
}

private fun mapApiTypeRemoteToLocal(apiType: String): ApiType =
    when (apiType) {
        EXTERNAL_API_HOP -> ApiType.HOP
        else -> ApiType.UNKNOWN
    }
