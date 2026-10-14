package io.paritytech.polkadotapp.chains.multiNetwork.chain.remote

import com.google.gson.Gson
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.model.ChainRemote
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.utils.fromJson
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

interface ChainFetcher {
    suspend fun getChains(): List<ChainRemote>
}

private const val CONFIG_CHAINS_KEY = "chains_v2"
private const val CONFIG_CHAINS_KEY_PRODUCTION = "chains"

class RemoteConfigChainFetcher @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
    private val gson: Gson,
    environment: TestnetEnvironment
) : ChainFetcher {
    private val chainsKey = when (environment) {
        TestnetEnvironment.PRODUCTION -> CONFIG_CHAINS_KEY_PRODUCTION
        TestnetEnvironment.TESTNET, TestnetEnvironment.NIGHTLY -> CONFIG_CHAINS_KEY
    }

    override suspend fun getChains(): List<ChainRemote> {
        val chainsConfig = getChainsRemote(chainsKey)
            ?: throw Throwable("Cannot fetch chains from Remote Config")
        return getChains(chainsConfig)
    }

    private suspend fun getChainsRemote(key: String) =
        remoteConfigService.getSyncedString(key).getOrNull()

    private fun getChains(config: String): List<ChainRemote> {
        return try {
            gson.fromJson<List<ChainRemote>>(config)
        } catch (e: Exception) {
            throw Throwable("Cannot parse chains from Remote Config", e)
        }
    }
}
