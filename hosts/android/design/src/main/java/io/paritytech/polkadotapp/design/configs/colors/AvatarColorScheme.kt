package io.paritytech.polkadotapp.design.configs.colors

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlin.math.absoluteValue

enum class AvatarColorScheme {
    Amethyst,
    Emerald,
    Garnet,
    Onyx,
    Opal,
    Pearl,
    Ruby,
    Sapphire,
    Topaz,
    Turquoise;

    val background: Color
        @Composable
        @ReadOnlyComposable
        get() = when (this) {
            Amethyst -> PolkadotTheme.colors.avatar.bg.amethyst
            Emerald -> PolkadotTheme.colors.avatar.bg.emerald
            Garnet -> PolkadotTheme.colors.avatar.bg.garnet
            Onyx -> PolkadotTheme.colors.avatar.bg.onyx
            Opal -> PolkadotTheme.colors.avatar.bg.opal
            Pearl -> PolkadotTheme.colors.avatar.bg.pearl
            Ruby -> PolkadotTheme.colors.avatar.bg.ruby
            Sapphire -> PolkadotTheme.colors.avatar.bg.sapphire
            Topaz -> PolkadotTheme.colors.avatar.bg.topaz
            Turquoise -> PolkadotTheme.colors.avatar.bg.turquoise
        }

    val foreground: Color
        @Composable
        @ReadOnlyComposable
        get() = when (this) {
            Amethyst -> PolkadotTheme.colors.avatar.fg.amethyst
            Emerald -> PolkadotTheme.colors.avatar.fg.emerald
            Garnet -> PolkadotTheme.colors.avatar.fg.garnet
            Onyx -> PolkadotTheme.colors.avatar.fg.onyx
            Opal -> PolkadotTheme.colors.avatar.fg.opal
            Pearl -> PolkadotTheme.colors.avatar.fg.pearl
            Ruby -> PolkadotTheme.colors.avatar.fg.ruby
            Sapphire -> PolkadotTheme.colors.avatar.fg.sapphire
            Topaz -> PolkadotTheme.colors.avatar.fg.topaz
            Turquoise -> PolkadotTheme.colors.avatar.fg.turquoise
        }

    companion object {
        fun from(key: ByteArray): AvatarColorScheme {
            val seed = key.contentHashCode().toLong()
            val index = (seed % entries.size).toInt().absoluteValue
            return entries[index]
        }
    }
}
