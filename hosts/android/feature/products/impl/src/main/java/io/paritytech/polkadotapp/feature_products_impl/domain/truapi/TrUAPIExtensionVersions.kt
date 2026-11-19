package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.feature_transactions.api.data.TransactionExtensionVersions
import uniffi.truapi.ChainExtensionVersion
import uniffi.truapi.ExtensionVersionMap
import javax.inject.Inject

class TrUAPIExtensionVersions @Inject constructor(
    private val transactionExtensionVersions: TransactionExtensionVersions,
    private val chainRegistry: ChainRegistry,
) {
    suspend fun resolve(): Result<ExtensionVersionMap> =
        transactionExtensionVersions.forGeneralTransactionChains().mapCatching { versions ->
            val chains = versions.map { (chainId, extensionVersion) ->
                val genesisHash = chainRegistry.getChain(chainId).genesisHash
                ChainExtensionVersion(genesisHash = genesisHash.value, extensionVersion = extensionVersion.toUByte())
            }
            ExtensionVersionMap(chains = chains)
        }
}
