package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthBarDefaults

/**
 * Transparent, overlaid at the very top like the system status indicators. [title] takes the
 * width [indicators] leave, so a long title ends before them, and starts near the edge: its lockup pads
 * itself, so a plate behind it lines up with the screen.
 */
@Composable
fun StatusBarRow(
    title: @Composable () -> Unit,
    indicators: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .height(ChainHealthBarDefaults.ContentHeight)
            .padding(start = PolkadotTheme.spacings.extraSmall, end = PolkadotTheme.spacings.medium),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier.weight(1f),
            contentAlignment = Alignment.CenterStart,
        ) {
            title()
        }

        indicators()
    }
}
