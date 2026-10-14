package io.paritytech.polkadotapp.feature_settings_api.domain.language

import kotlinx.coroutines.flow.Flow

/**
 * The app's current locale as a BCP 47 language tag (e.g. "en", "es-ES"): the per-app language
 * chosen in system settings, otherwise the device language. It may name a language the app itself
 * has no translation for.
 * Emits the current value on subscribe and again on every change.
 */
interface AppLanguageProvider {
    val languageTag: Flow<String>
}
