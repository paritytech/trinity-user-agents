package io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import io.paritytech.polkadotapp.design.colors.LegacyNovaStableColors
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenDOT
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDC
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDT
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplayId
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig

val LocalKnownTokenFormatter = staticCompositionLocalOf<KnownTokenFormatter> {
    noLocalProvidedFor("KnownTokenFormatter")
}

interface KnownTokenFormatter {
    fun appearanceOf(display: AssetDisplay): KnownTokenUiConfig

    companion object {
        val mocked: KnownTokenFormatter get() = MockedKnownTokenFormatter()
    }
}

private class MockedKnownTokenFormatter : KnownTokenFormatter {
    override fun appearanceOf(display: AssetDisplay): KnownTokenUiConfig {
        val color: Color
        val icon: ImageVector

        when (display.displayId) {
            AssetDisplayId.DOT -> {
                color = LegacyNovaStableColors.PinkPink600
                icon = NovaIcons.TokenDOT
            }
            AssetDisplayId.USDC -> {
                color = LegacyNovaStableColors.BlueBlue600
                icon = NovaIcons.TokenUSDC
            }
            AssetDisplayId.USDT -> {
                color = LegacyNovaStableColors.AdvancedTurquoiseTurquoise600
                icon = NovaIcons.TokenUSDT
            }
            AssetDisplayId.PAS -> {
                color = LegacyNovaStableColors.BlueBlue600
                icon = NovaIcons.TokenDOT
            }
        }

        return KnownTokenUiConfig(
            color = color,
            icon = icon,
            symbol = display.asset.symbol,
            name = display.asset.name
        )
    }
}
