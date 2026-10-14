package io.paritytech.polkadotapp.feature_dotns_impl.data.config

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsConfig
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import timber.log.Timber
import javax.inject.Inject

interface DotNsConfigProvider {
    suspend fun getDotNsConfig(): Result<DotNsConfig>
}

internal class RemoteConfigDotNsConfigProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : DotNsConfigProvider {
    private companion object {
        const val CONFIG_KEY = "dot_ns_config"
    }

    override suspend fun getDotNsConfig(): Result<DotNsConfig> {
        return remoteConfigService.getSyncedJsonObject<DotNsConfigRemote>(CONFIG_KEY)
            .mapCatching { it.toDomain() }
    }

    private fun DotNsConfigRemote.toDomain(): DotNsConfig {
        val registry = registryContractAddress?.takeIf { it.isNotEmpty() }
        // Legacy-only mode is a supported configuration, so this is a diagnostic, not a warning.
        if (registry == null) {
            Timber.d("No dotNS registry address in $CONFIG_KEY — resolving legacy names only")
        }

        return DotNsConfig(
            resolverContractAddress = resolverContractAddress.hexToDataByteArray(),
            registryContractAddress = registry?.hexToDataByteArray(),
        )
    }
}
