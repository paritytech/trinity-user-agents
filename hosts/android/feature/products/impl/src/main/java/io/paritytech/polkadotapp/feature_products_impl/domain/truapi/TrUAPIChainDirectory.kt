package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.flow.first
import uniffi.truapi.ChainIdentifier
import uniffi.truapi.HostChainEntry
import uniffi.truapi.HostChainSet
import javax.inject.Inject

/**
 * What chains the host serves and how to reach them. RFC-0026 requires the two
 * to agree, and the core asks for them on a thread where it cannot block, so
 * both are resolved once per session.
 */
class TrUAPIChainDirectory @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val hostApiInteractor: HostApiInteractor,
    private val knownChains: KnownChains,
    private val environment: TestnetEnvironment,
) {
    suspend fun resolve(): TrUAPIChains {
        // One read backs both halves; two reads let them disagree.
        val chains = runCatching { chainRegistry.currentChains.first() }.getOrDefault(emptyList())

        // Every registered chain, not just the advertised roles: `chainConnect`
        // and `featureSupported` both answer from this map, so a product can
        // reach a chain the host carries without it holding a named role.
        val endpoints = chains.associate { chain ->
            chain.genesisHash.hexKey() to
                hostApiInteractor.chainNodes(chain.genesisHash).getOrDefault(emptyList())
        }

        return TrUAPIChains(
            advertised = advertised(chains, endpoints),
            endpoints = endpoints,
        )
    }

    private fun advertised(chains: List<Chain>, endpoints: Map<String, List<String>>): HostChainSet {
        val byId = chains.associateBy { it.id }
        // The relay is AssetHub's parent rather than a KnownChains slot.
        val relayId = byId[knownChains.assetHub]?.parentId
        val roles = listOfNotNull(
            relayId?.let { ChainIdentifier.RELAY to it },
            ChainIdentifier.ASSET_HUB to knownChains.assetHub,
            ChainIdentifier.PEOPLE to knownChains.people,
            ChainIdentifier.BULLETIN to knownChains.bulletIn,
        ).mapNotNull { (identifier, chainId) ->
            byId[chainId]?.let { identifier to it.genesisHash }
        }

        return advertisedFrom(roles, endpoints, environment.ecosystem())
    }
}

/** Never advertise a chain `chainConnect` cannot dial: no endpoints, no entry. */
internal fun advertisedFrom(
    roles: List<Pair<ChainIdentifier, GenesisHash>>,
    endpoints: Map<String, List<String>>,
    network: String,
): HostChainSet = HostChainSet(
    network = network,
    chains = roles.mapNotNull { (identifier, genesisHash) ->
        if (endpoints[genesisHash.hexKey()].isNullOrEmpty()) null
        else HostChainEntry(identifier, genesisHash.value)
    },
)

class TrUAPIChains(
    val advertised: HostChainSet,
    /** Genesis hash, lower-case hex, to its JSON-RPC endpoints in preference order. */
    val endpoints: Map<String, List<String>>,
) {
    /**
     * Whether `chainConnect` will dial this chain. Also the answer to
     * `featureSupported(Chain)`, so the host cannot promise a chain it refuses.
     */
    fun canDial(genesisHash: ByteArray): Boolean = !endpoints[genesisHash.hexKey()].isNullOrEmpty()
}

internal val EMPTY_CHAINS = TrUAPIChains(
    advertised = HostChainSet(network = "", chains = emptyList()),
    endpoints = emptyMap(),
)

@OptIn(ExperimentalStdlibApi::class)
internal fun GenesisHash.hexKey(): String = value.toHexString()

@OptIn(ExperimentalStdlibApi::class)
internal fun ByteArray.hexKey(): String = toHexString()

/**
 * RFC-0026 `network` is an open ecosystem string, not a mainnet/testnet flag.
 * Both app testnets are Paseo-based and the core itself reports "paseo" for
 * them; split these apart if a product ever needs to tell nightly from preview.
 */
private fun TestnetEnvironment.ecosystem(): String = when (this) {
    TestnetEnvironment.PRODUCTION -> "polkadot"
    TestnetEnvironment.NIGHTLY, TestnetEnvironment.TESTNET -> "paseo"
}
