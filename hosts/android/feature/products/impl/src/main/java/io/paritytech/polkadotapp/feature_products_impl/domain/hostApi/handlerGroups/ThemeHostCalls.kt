package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ThemeVariant
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import kotlinx.coroutines.flow.map

class ThemeHostCalls(
    private val botApi: ProductsBotApi,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerSubscription<Unit, ThemeDto>("themeSubscribe") {
            botApi.subscribeTheme().map(ProductTheme::toDto)
        }
    }
}

private fun ProductTheme.toDto(): ThemeDto = ThemeDto(
    name = ThemeNameDto(tag = "Custom", value = name),
    variant = when (variant) {
        ThemeVariant.Light -> "Light"
        ThemeVariant.Dark -> "Dark"
    },
)

private data class ThemeDto(val name: ThemeNameDto, val variant: String)

private data class ThemeNameDto(val tag: String, val value: String)
