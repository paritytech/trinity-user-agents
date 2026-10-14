package io.paritytech.polkadotapp.design.components.button.default

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.text.TextStyle
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

interface PolkadotButtonSize {
    val padding: PaddingValues
    val textStyle: TextStyle

    companion object {
        @Composable
        fun largeIncreased(): PolkadotButtonSize = rememberButtonSize(
            padding = PaddingValues(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.mediumIncreased
            ),
            textStyle = PolkadotTheme.typography.title.large
        )

        @Composable
        fun large(): PolkadotButtonSize = rememberButtonSize(
            padding = PaddingValues(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
            textStyle = PolkadotTheme.typography.title.medium
        )

        @Composable
        fun mediumIncreased(): PolkadotButtonSize = rememberButtonSize(
            padding = PaddingValues(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
            textStyle = PolkadotTheme.typography.title.medium
        )

        @Composable
        fun medium(): PolkadotButtonSize = rememberButtonSize(
            padding = PaddingValues(
                horizontal = PolkadotTheme.spacings.mediumIncreased,
                vertical = PolkadotTheme.spacings.smallIncreased
            ),
            textStyle = PolkadotTheme.typography.title.medium
        )

        @Composable
        @Deprecated("Not a part of design system. Should be removed after all consumers are gone.")
        fun custom(
            padding: PaddingValues,
            textStyle: TextStyle = PolkadotTheme.typography.title.medium
        ): PolkadotButtonSize = rememberButtonSize(padding = padding, textStyle = textStyle)

        @Composable
        private fun rememberButtonSize(
            padding: PaddingValues,
            textStyle: TextStyle
        ): PolkadotButtonSize = remember(padding, textStyle) {
            RealPolkadotButtonSize(padding, textStyle)
        }
    }
}

@Immutable
private data class RealPolkadotButtonSize(
    override val padding: PaddingValues,
    override val textStyle: TextStyle
) : PolkadotButtonSize
