package io.paritytech.polkadotapp.design.components.button.icon

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

interface PolkadotIconButtonSize {
    val padding: PaddingValues
    val iconSize: Dp

    companion object {
        @Composable
        fun extraLarge(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.mediumIncreased),
            iconSize = 32.dp
        )

        @Composable
        fun mediumIncreased(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.extraMedium),
            iconSize = 24.dp
        )

        @Composable
        fun medium(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.smallIncreased),
            iconSize = 24.dp
        )

        @Composable
        fun small(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.small),
            iconSize = 20.dp
        )

        @Composable
        fun extraSmall(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.extraSmall),
            iconSize = 20.dp
        )

        @Composable
        fun tiny(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.extraTiny),
            iconSize = 16.dp
        )

        @Composable
        fun extraTiny(): PolkadotIconButtonSize = rememberIconButtonSize(
            padding = PaddingValues(PolkadotTheme.spacings.extraTiny),
            iconSize = 16.dp
        )

        @Composable
        private fun rememberIconButtonSize(
            padding: PaddingValues,
            iconSize: Dp
        ): PolkadotIconButtonSize = remember(padding, iconSize) {
            RealPolkadotIconButtonSize(padding, iconSize)
        }
    }
}

@Immutable
private data class RealPolkadotIconButtonSize(
    override val padding: PaddingValues,
    override val iconSize: Dp
) : PolkadotIconButtonSize
