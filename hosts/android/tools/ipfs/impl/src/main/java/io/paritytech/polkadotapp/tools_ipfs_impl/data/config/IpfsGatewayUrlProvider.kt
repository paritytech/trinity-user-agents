package io.paritytech.polkadotapp.tools_ipfs_impl.data.config

import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

internal interface IpfsGatewayUrlProvider {
    suspend fun getGatewayUrl(): Result<String>
}

internal class RemoteConfigIpfsGatewayUrlProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : IpfsGatewayUrlProvider {
    private companion object {
        const val CONFIG_KEY = "ipfs_gateway_url"
    }

    override suspend fun getGatewayUrl(): Result<String> {
        return remoteConfigService.getSyncedString(CONFIG_KEY)
    }
}
