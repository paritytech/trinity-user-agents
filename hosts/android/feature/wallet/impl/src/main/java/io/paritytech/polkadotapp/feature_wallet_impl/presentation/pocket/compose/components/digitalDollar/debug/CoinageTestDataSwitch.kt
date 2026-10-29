package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.debug

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageScene
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.debug.CoinageTestDataMode
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun CoinageTestDataSwitch(
    modifier: Modifier = Modifier,
    mode: CoinageTestDataMode,
    onModeChange: (CoinageTestDataMode) -> Unit
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        CoinageTestDataMode.entries.forEach { option ->
            val selected = option == mode

            NovaText(
                modifier = Modifier.selectable(
                    selected = selected,
                    role = Role.RadioButton,
                    onClick = { onModeChange(option) }
                ),
                text = (if (selected) "● " else "○ ") + stringResource(option.title),
                maxLines = 1,
                style = PolkadotTheme.typography.body.small,
                color = if (selected) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary
            )
        }
    }
}

@Composable
internal fun CoinageCoinSetSwitch(
    modifier: Modifier = Modifier,
    set: CoinageScene.CoinSet,
    onSetChange: (CoinageScene.CoinSet) -> Unit
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            text = stringResource(RCommon.string.pocket_debug_coinage_coin_set),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.primary
        )

        Row(
            modifier = Modifier
                .border(1.dp, PolkadotTheme.colors.fg.secondary, CircleShape)
                .clip(CircleShape)
                .selectableGroup()
        ) {
            CoinageScene.CoinSet.entries.forEach { option ->
                val selected = option == set

                NovaText(
                    modifier = Modifier
                        .background(if (selected) PolkadotTheme.colors.fg.primary else Color.Transparent)
                        .selectable(selected = selected, role = Role.RadioButton, onClick = { onSetChange(option) })
                        .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.tiny),
                    text = stringResource(option.title),
                    style = PolkadotTheme.typography.body.small,
                    color = if (selected) PolkadotTheme.colors.bg.surface.container else PolkadotTheme.colors.fg.primary
                )
            }
        }
    }
}

private val CoinageScene.CoinSet.title: Int
    get() = when (this) {
        CoinageScene.CoinSet.VARIED -> RCommon.string.pocket_debug_coinage_coin_set_varied
        CoinageScene.CoinSet.PLAIN -> RCommon.string.pocket_debug_coinage_coin_set_plain
    }

internal val CoinageTestDataMode.title: Int
    get() = when (this) {
        CoinageTestDataMode.LIVE -> RCommon.string.pocket_debug_coinage_test_live
        CoinageTestDataMode.EVERY_DENOMINATION -> RCommon.string.pocket_debug_coinage_test_values
        CoinageTestDataMode.EVERY_STATUS -> RCommon.string.pocket_debug_coinage_test_states
        CoinageTestDataMode.RANDOM_FIFTY -> RCommon.string.pocket_debug_coinage_test_fifty
        CoinageTestDataMode.RANDOM_FIVE_HUNDRED -> RCommon.string.pocket_debug_coinage_test_five_hundred
    }
