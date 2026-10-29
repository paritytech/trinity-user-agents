package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey

private val KEY_ROWS = listOf(
    listOf(FundingKey.Digit('1'), FundingKey.Digit('2'), FundingKey.Digit('3')),
    listOf(FundingKey.Digit('4'), FundingKey.Digit('5'), FundingKey.Digit('6')),
    listOf(FundingKey.Digit('7'), FundingKey.Digit('8'), FundingKey.Digit('9')),
    listOf(FundingKey.Point, FundingKey.Digit('0'), FundingKey.Delete),
)

private val KEY_HEIGHT = 48.dp
private val DELETE_ICON_SIZE = 20.dp

/** Digits, a decimal point and delete. The text it edits is the view model's. */
@Composable
fun FundingKeypad(
    onKey: (FundingKey) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        KEY_ROWS.forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                row.forEach { key ->
                    PolkadotSurface(
                        modifier = Modifier
                            .weight(1f)
                            .height(KEY_HEIGHT),
                        shape = PolkadotTheme.shapes.full,
                        color = PolkadotTheme.colors.bg.surface.container,
                        contentAlignment = Alignment.Center,
                        onClick = { onKey(key) },
                    ) {
                        when (key) {
                            is FundingKey.Digit -> KeyLabel(key.digit.toString())
                            FundingKey.Point -> KeyLabel(".")
                            FundingKey.Delete -> NovaIcon(
                                modifier = Modifier.height(DELETE_ICON_SIZE),
                                imageVector = NovaIcons.ArrowLeft,
                                tint = PolkadotTheme.colors.fg.primary,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun KeyLabel(text: String) {
    NovaText(
        text = text,
        style = PolkadotTheme.typography.title.large,
        color = PolkadotTheme.colors.fg.primary,
    )
}
