package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Whether this build's own network runs on the embedded light client. Off by default. Read once when
 * the app starts, so a change applies after a restart.
 */
@Singleton
class LightClientSettings @Inject constructor(
    @param:ApplicationContext context: Context,
) {
    private val prefs = context.getSharedPreferences("light_client_settings", Context.MODE_PRIVATE)

    fun isEnabled(): Boolean = prefs.getBoolean(KEY_ENABLED, false)

    // Committed, not applied: the change only takes effect through a restart, which can follow at once.
    fun setEnabled(enabled: Boolean) = prefs.edit(commit = true) { putBoolean(KEY_ENABLED, enabled) }

    private companion object {
        const val KEY_ENABLED = "enabled"
    }
}
