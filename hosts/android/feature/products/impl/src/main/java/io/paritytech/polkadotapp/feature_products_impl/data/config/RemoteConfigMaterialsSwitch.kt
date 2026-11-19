package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

class RemoteConfigMaterialsSwitch @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) {
    // Not the synced read: it waits for a fetch with no deadline, and the runtime boot would wait with it.
    suspend fun isOn(): Boolean = remoteConfigService.getBoolean(KEY)
        .logFailure("Remote Config $KEY unreadable; materials stay off")
        .getOrDefault(false)

    private companion object {
        const val KEY = "materials_enabled"
    }
}
