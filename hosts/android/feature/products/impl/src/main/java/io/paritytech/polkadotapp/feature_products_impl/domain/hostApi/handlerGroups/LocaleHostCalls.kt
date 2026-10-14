package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.flow.map

class LocaleHostCalls(
    private val appLanguageProvider: AppLanguageProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerSubscription<Unit, LocaleDto>("localeSubscribe") {
            appLanguageProvider.languageTag.map { LocaleDto(languageTag = it) }
        }
    }
}

private data class LocaleDto(val languageTag: String)
