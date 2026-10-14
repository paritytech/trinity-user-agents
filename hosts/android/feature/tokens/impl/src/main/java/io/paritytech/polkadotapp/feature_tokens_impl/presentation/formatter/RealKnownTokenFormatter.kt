package io.paritytech.polkadotapp.feature_tokens_impl.presentation.formatter

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.design.colors.LegacyNovaStableColors
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenDOT
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDC
import io.paritytech.polkadotapp.design.components.icon.vectors.TokenUSDT
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplayId
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealKnownTokenFormatter @Inject constructor() : KnownTokenFormatter {
    companion object {
    }

    override fun appearanceOf(display: AssetDisplay): KnownTokenUiConfig {
        return when (display.displayId) {
            AssetDisplayId.USDT -> display.asset.usdtAppearance()
            AssetDisplayId.USDC -> display.asset.usdcAppearance()
            AssetDisplayId.DOT -> display.asset.dotAppearance()
            AssetDisplayId.PAS -> display.asset.pasAppearance()
        }
    }

    private fun Chain.Asset.dotAppearance(): KnownTokenUiConfig {
        return KnownTokenUiConfig(
            color = LegacyNovaStableColors.PinkPink600,
            icon = NovaIcons.TokenDOT,
            symbol = symbol,
            name = name
        )
    }

    private fun Chain.Asset.usdtAppearance(): KnownTokenUiConfig {
        return KnownTokenUiConfig(
            color = LegacyNovaStableColors.AdvancedTurquoiseTurquoise600,
            icon = NovaIcons.TokenUSDT,
            symbol = symbol,
            name = name
        )
    }

    private fun Chain.Asset.usdcAppearance(): KnownTokenUiConfig {
        return KnownTokenUiConfig(
            color = LegacyNovaStableColors.BlueBlue600,
            icon = NovaIcons.TokenUSDC,
            symbol = symbol,
            name = name
        )
    }

    private fun Chain.Asset.pasAppearance(): KnownTokenUiConfig {
        return KnownTokenUiConfig(
            color = LegacyNovaStableColors.BlueBlue600,
            icon = NovaIcons.TokenDOT,
            symbol = symbol,
            name = name
        )
    }
}
