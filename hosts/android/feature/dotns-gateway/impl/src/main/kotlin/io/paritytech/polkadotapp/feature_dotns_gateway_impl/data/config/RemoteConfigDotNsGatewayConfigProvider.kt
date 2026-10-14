package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import javax.inject.Inject

internal class RemoteConfigDotNsGatewayConfigProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService
) : DotNsGatewayConfigProvider {
    private companion object {
        const val CONFIG_KEY = "dot_ns_config"
    }

    override suspend fun getConfig(): Result<DotNsGatewayConfig> {
        return remoteConfigService.getSyncedJsonObject<DotNsGatewayConfigRemote>(CONFIG_KEY)
            .mapCatching { it.toDomain() }
    }

    private fun DotNsGatewayConfigRemote.toDomain(): DotNsGatewayConfig {
        return DotNsGatewayConfig(
            popControllerAddress = popControllerAddress.hexToDataByteArray(),
            popResolverAddress = popResolverAddress.hexToDataByteArray()
        )
    }
}
