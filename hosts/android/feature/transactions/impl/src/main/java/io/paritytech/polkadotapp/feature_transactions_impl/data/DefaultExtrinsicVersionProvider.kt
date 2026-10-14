package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.novasama.substrate_sdk_android.runtime.extrinsic.ExtrinsicVersion
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.VerifySignature
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import javax.inject.Inject

interface DefaultExtrinsicVersionProvider {
    suspend fun getDefaultExtrinsicVersion(chainId: ChainId, isSigned: Boolean): Result<ExtrinsicVersion>
}

class RealDefaultExtrinsicVersionProvider @Inject constructor(
    private val knownChains: KnownChains,
    private val remoteConfigService: RemoteConfigService,
    private val chainRegistry: ChainRegistry,
) : DefaultExtrinsicVersionProvider {
    private companion object {
        const val TX_EXTENSION_VERSIONS_KEY = "transaction_extension_versions"
        const val TX_EXTENSION_VERSION_DEFAULT = 0
    }

    override suspend fun getDefaultExtrinsicVersion(chainId: ChainId, isSigned: Boolean): Result<ExtrinsicVersion> {
        if (chainId != knownChains.people && chainId != knownChains.assetHub) return Result.success(ExtrinsicVersion.V4)

        return getTransactionExtensionVersion(chainId).mapCatching { extensionVersion ->
            if (isSigned && !verifiesGeneralSignatures(chainId, extensionVersion)) {
                ExtrinsicVersion.V4
            } else {
                ExtrinsicVersion.V5(extensionVersion)
            }
        }
    }

    // A signed general transaction carries its signature in VerifySignature, so a pipeline without it cannot take
    // one. Such runtimes still accept v4 — but v4 always runs pipeline 0, whatever extensions later versions add.
    private suspend fun verifiesGeneralSignatures(chainId: ChainId, extensionVersion: Byte): Boolean {
        return chainRegistry.getRuntime(chainId).metadata.extrinsic
            .transactionExtensions(extensionVersion.toInt())
            .any { it.id == VerifySignature.ID }
    }

    private suspend fun getTransactionExtensionVersion(chainId: ChainId): Result<Byte> {
        // Gson receives only the raw Class, so map values arrive as Double regardless of the declared type
        return remoteConfigService.getSyncedJsonObject<Map<String, Number>>(TX_EXTENSION_VERSIONS_KEY)
            .map { versions -> (versions[chainId] ?: TX_EXTENSION_VERSION_DEFAULT).toByte() }
    }
}
