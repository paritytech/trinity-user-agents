package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import javax.inject.Inject

interface FundingProvidersConfig {
    fun publishedNames(): Flow<List<String>>
}

internal class RemoteConfigFundingProvidersConfig @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : FundingProvidersConfig {
    // A value that does not decode keeps the list the core had.
    override fun publishedNames(): Flow<List<String>> = remoteConfigService.observeString(CONFIG_KEY).mapNotNull { raw ->
        runCatching { json.decodeFromString<FundingConfigRemote>(raw) }
            .logFailure("Unreadable $CONFIG_KEY")
            .getOrNull()
            ?.providers
    }

    @Serializable
    private class FundingConfigRemote(
        val providers: List<String>,
    )

    private companion object {
        const val CONFIG_KEY = "funding_config"

        val json = Json { ignoreUnknownKeys = true }
    }
}
