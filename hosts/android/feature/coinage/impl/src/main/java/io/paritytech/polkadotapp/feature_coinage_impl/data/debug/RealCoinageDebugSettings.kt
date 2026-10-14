package io.paritytech.polkadotapp.feature_coinage_impl.data.debug

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_coinage_api.domain.debug.CoinageDebugSettings
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealCoinageDebugSettings @Inject constructor(
    factory: SingleValueStorageFactory,
) : CoinageDebugSettings {
    private val storage = factory.preferences(
        key = "coinage_debug_widgets_enabled",
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.toString() },
            fromString = { it.toBoolean() },
        ),
        default = DEFAULT_ENABLED,
    )

    override fun widgetsEnabledFlow(): Flow<Boolean> = storage.valueFlow().map { it ?: DEFAULT_ENABLED }

    override suspend fun areWidgetsEnabled(): Boolean = storage.requireValue()

    override suspend fun setWidgetsEnabled(enabled: Boolean) = storage.saveValue(enabled)

    private companion object {
        const val DEFAULT_ENABLED = true
    }
}
