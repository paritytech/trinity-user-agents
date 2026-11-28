package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsDebugSettings
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsOverride
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

/** Whether the core draws materials: the Remote Config switch, unless the debug menu overrides it. */
class RemoteConfigMaterialsSwitch @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
    private val debugSettings: MaterialsDebugSettings,
) {
    suspend fun isOn(): Boolean = when (debugSettings.override()) {
        MaterialsOverride.ON -> true
        MaterialsOverride.OFF -> false
        MaterialsOverride.REMOTE_CONFIG -> isOnRemotely()
    }

    // Not the synced read: it waits for a fetch with no deadline, and the runtime boot would wait with it.
    private suspend fun isOnRemotely(): Boolean = remoteConfigService.getBoolean(KEY)
        .logFailure("Remote Config $KEY unreadable; materials stay off")
        .getOrDefault(false)

    private companion object {
        const val KEY = "materials_enabled"
    }
}
