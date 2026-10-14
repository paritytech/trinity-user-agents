package io.paritytech.polkadotapp.feature_dotns_impl.data.contract

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.feature_dotns_impl.data.config.DotNsConfigProvider
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.abi.EvmContractCaller
import io.paritytech.polkadotapp.feature_revive_api.NameHash
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import javax.inject.Inject

class RealDotNsContractApi @Inject constructor(
    private val reviveContractApi: ReviveContractApi,
    private val chainRegistry: ChainRegistry,
    private val dotNsConfigProvider: DotNsConfigProvider,
) : DotNsContractApi {
    override suspend fun resolveContentHash(dotNsName: String): Result<ByteArray?> {
        val node = NameHash.nameHash(dotNsName)
        val callData = EvmContractCaller.encodeContentHash(node)

        return dotNsConfigProvider.getDotNsConfig()
            .flatMap { config -> readContentHash(config, dotNsName, callData) }
            .map { outputBytes ->
                val contentHash = if (outputBytes.isEmpty()) null else EvmContractCaller.decodeContentHash(outputBytes)
                contentHash?.let(::stripEip1577Prefix)
            }
    }

    // A name's own resolver serves its records and need not carry a content hash - the hash then
    // stays on the fixed resolver, so an empty answer from a registry one is retried there.
    private suspend fun readContentHash(
        config: DotNsConfig,
        dotNsName: String,
        callData: ByteArray,
    ): Result<ByteArray> {
        return contentResolverFor(config, dotNsName).flatMap { resolver ->
            readRecord(callData, resolver).flatMap { outputBytes ->
                if (outputBytes.isNotEmpty() || resolver == config.resolverContractAddress) {
                    Result.success(outputBytes)
                } else {
                    readRecord(callData, config.resolverContractAddress)
                }
            }
        }
    }

    override suspend fun getMetadata(dotNsName: String, key: String): Result<String?> {
        val node = NameHash.nameHash(dotNsName)
        val callData = EvmContractCaller.encodeText(node, key)

        return dotNsConfigProvider.getDotNsConfig().flatMap { config ->
            registryResolverOverrideFor(config, dotNsName).flatMap { resolver ->
                // Text records live only on a registry resolver, so a name without one has none.
                if (resolver == null) {
                    Result.success(null)
                } else {
                    readRecord(callData, resolver).map { outputBytes ->
                        if (outputBytes.isEmpty()) null else EvmContractCaller.decodeText(outputBytes)
                    }
                }
            }
        }
    }

    // Legacy names have no registry entry and are served by the fixed content-resolver.
    private suspend fun contentResolverFor(config: DotNsConfig, dotNsName: String): Result<AccountId> {
        return registryResolverOverrideFor(config, dotNsName).map { it ?: config.resolverContractAddress }
    }

    // Null means the name has no registry entry. A failure means the registry call itself failed,
    // which is distinct from absent.
    private suspend fun registryResolverOverrideFor(config: DotNsConfig, dotNsName: String): Result<AccountId?> {
        // An unconfigured registry disables manifest resolution; it must not break legacy names,
        // which never had a registry entry to begin with.
        val registry = config.registryContractAddress ?: return Result.success(null)

        val callData = EvmContractCaller.encodeResolver(NameHash.nameHash(dotNsName))

        return callContract(callData, registry).map { outputBytes ->
            EvmContractCaller.decodeAddress(outputBytes)?.intoAccountId()
        }
    }

    // A resolver without the record, or without the call at all, reverts rather than answering empty.
    private suspend fun readRecord(inputData: ByteArray, resolver: AccountId): Result<ByteArray> {
        return callContract(inputData, resolver).flatRecover { error ->
            if (error is ReviveContractReverted) Result.success(ByteArray(0)) else Result.failure(error)
        }
    }

    private suspend fun callContract(inputData: ByteArray, dest: AccountId): Result<ByteArray> {
        return reviveContractApi.callReadOnly(
            chainId = chainRegistry.knownChains.assetHub,
            contract = dest,
            input = inputData.toDataByteArray(),
        ).map { it.value }
    }

    /**
     * Strips the EIP-1577 uvarint-encoded multicodec prefix from a content hash.
     *
     * The `contenthash()` resolver returns an EIP-1577 encoded value where the first bytes
     * identify the storage system. IPFS namespace (0xe3) is uvarint-encoded as 2 bytes: `e3 01`.
     * The remaining bytes are the raw CID.
     */
    private fun stripEip1577Prefix(contentHash: ByteArray): ByteArray {
        require(contentHash.size > EIP_1577_IPFS_PREFIX.size) { "Content hash too short" }

        val prefix = contentHash.copyOfRange(0, EIP_1577_IPFS_PREFIX.size)
        require(prefix.contentEquals(EIP_1577_IPFS_PREFIX)) {
            "Unsupported EIP-1577 prefix: 0x${prefix.joinToString("") { "%02x".format(it) }}"
        }

        return contentHash.copyOfRange(EIP_1577_IPFS_PREFIX.size, contentHash.size)
    }

    companion object {
        private val EIP_1577_IPFS_PREFIX = byteArrayOf(0xe3.toByte(), 0x01)
    }
}
