package io.paritytech.polkadotapp.feature_coinage_impl.data.config

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinagePrivacyConfig
import io.paritytech.polkadotapp.feature_coinage_impl.data.mappers.toDomain
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getJsonObject
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface CoinagePrivacyConfigProvider {
    suspend fun privacyConfigOrDefault(): CoinagePrivacyConfig

    /**
     * What is activated now, then whatever the first sync activates — a balance never waits on the network,
     * and a config that arrives mid-session reaches it without waiting for the next wallet update.
     */
    fun privacyConfigUpdates(): Flow<CoinagePrivacyConfig>
}

class RemoteConfigCoinagePrivacyConfigProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
    private val dispatchers: CoroutineDispatchers,
) : CoinagePrivacyConfigProvider {
    override suspend fun privacyConfigOrDefault(): CoinagePrivacyConfig = withContext(dispatchers.io) {
        // An unset key reads back as an empty string, which Gson parses to null.
        remoteConfigService.getJsonObject<CoinagePrivacyConfigRemote?>(CONFIG_KEY)
            .logFailure(READ_FAILURE)
            .getOrNull()
            .toDomain()
    }

    override fun privacyConfigUpdates(): Flow<CoinagePrivacyConfig> = flow {
        emit(privacyConfigOrDefault())
        emit(syncedPrivacyConfigOrDefault())
    }.distinctUntilChanged()

    private suspend fun syncedPrivacyConfigOrDefault(): CoinagePrivacyConfig = withContext(dispatchers.io) {
        remoteConfigService.getSyncedJsonObject<CoinagePrivacyConfigRemote?>(CONFIG_KEY)
            .logFailure(READ_FAILURE)
            .getOrNull()
            .toDomain()
    }

    private companion object {
        const val CONFIG_KEY = "coinage_privacy_config"
        const val READ_FAILURE = "Can't read the coinage privacy config, using the bundled defaults"
    }
}
