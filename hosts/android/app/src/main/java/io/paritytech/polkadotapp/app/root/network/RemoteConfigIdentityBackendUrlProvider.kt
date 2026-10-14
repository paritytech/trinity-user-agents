package io.paritytech.polkadotapp.app.root.network

import io.paritytech.polkadotapp.common.data.network.IdentityBackendUrlProvider
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

class RemoteConfigIdentityBackendUrlProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : IdentityBackendUrlProvider {
    private companion object {
        const val CONFIG_KEY = "identity_backend_url"
    }

    override suspend fun getBaseUrl(): Result<String> {
        return remoteConfigService.getSyncedString(CONFIG_KEY)
    }
}
