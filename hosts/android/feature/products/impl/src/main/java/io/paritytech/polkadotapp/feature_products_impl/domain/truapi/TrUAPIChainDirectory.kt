package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.flow.first
import javax.inject.Inject

/**
 * How to reach the chains the host carries. The core asks on a thread where it
 * cannot block, so the endpoints are resolved once per session.
 */
class TrUAPIChainDirectory @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val hostApiInteractor: HostApiInteractor,
) {
    suspend fun resolve(): TrUAPIChains {
        val chains = runCatching { chainRegistry.currentChains.first() }.getOrDefault(emptyList())

        // Every registered chain: `chainConnect` and `featureSupported` both
        // answer from this map, so a product can reach any chain the host carries.
        val endpoints = chains.associate { chain ->
            chain.genesisHash.hexKey() to
                hostApiInteractor.chainNodes(chain.genesisHash).getOrDefault(emptyList())
        }

        return TrUAPIChains(endpoints = endpoints)
    }
}

class TrUAPIChains(
    /** Genesis hash, lower-case hex, to its JSON-RPC endpoints in preference order. */
    val endpoints: Map<String, List<String>>,
) {
    /**
     * Whether `chainConnect` will dial this chain. Also the answer to
     * `featureSupported(Chain)`, so the host cannot promise a chain it refuses.
     */
    fun canDial(genesisHash: ByteArray): Boolean = !endpoints[genesisHash.hexKey()].isNullOrEmpty()
}

internal val EMPTY_CHAINS = TrUAPIChains(endpoints = emptyMap())

@OptIn(ExperimentalStdlibApi::class)
internal fun GenesisHash.hexKey(): String = value.toHexString()

@OptIn(ExperimentalStdlibApi::class)
internal fun ByteArray.hexKey(): String = toHexString()
