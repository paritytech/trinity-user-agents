package io.paritytech.polkadotapp.feature_settings_impl.data.repository

import android.content.ComponentCallbacks
import android.content.Context
import android.content.res.Configuration
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import javax.inject.Inject

class RealAppLanguageProvider @Inject constructor(
    @ApplicationContext private val context: Context,
) : AppLanguageProvider {
    override val languageTag: Flow<String> = callbackFlow {
        val callbacks = object : ComponentCallbacks {
            override fun onConfigurationChanged(newConfig: Configuration) {
                trySend(newConfig.languageTag())
            }

            @Deprecated("Deprecated in Java")
            override fun onLowMemory() = Unit
        }

        context.registerComponentCallbacks(callbacks)
        trySend(context.resources.configuration.languageTag())

        awaitClose { context.unregisterComponentCallbacks(callbacks) }
    }.distinctUntilChanged()

    private fun Configuration.languageTag(): String = locales[0].toLanguageTag()
}
