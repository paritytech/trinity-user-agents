package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MockChatProductSettings
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow

class PrefsMockChatProductSettings(
    private val prefs: SharedPreferences,
) : MockChatProductSettings {
    private val enabled = MutableStateFlow(prefs.getBoolean(KEY_ENABLED, false))

    override fun observeEnabled(): Flow<Boolean> = enabled

    override fun setEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_ENABLED, enabled).apply()
        this.enabled.value = enabled
    }

    private companion object {
        const val KEY_ENABLED = "mock_chat_product_enabled"
    }
}
