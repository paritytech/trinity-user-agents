package io.paritytech.polkadotapp.feature_device_sync_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.feature_device_sync_api.domain.DeviceSyncCoordinator
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class DeviceSyncInitializer @Inject constructor(
    private val coordinator: DeviceSyncCoordinator,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        if (FeatureOption.LINKED_DEVICES.isDisabled) return@runCatching

        coordinator.startSubscriptions()
    }
}
