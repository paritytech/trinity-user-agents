package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig

@Composable
fun TokenChip(tokenUiConfig: KnownTokenUiConfig) {
    Row(
        modifier = Modifier
            .background(
                shape = PolkadotTheme.shapes.mediumIncreased,
                color = Color(0x1FFFFFFF)
            )
            .padding(4.dp, 4.dp, 12.dp, 4.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Image(
            modifier = Modifier.size(24.dp),
            imageVector = tokenUiConfig.icon,
            contentDescription = "token_icon"
        )
        HorizontalSpacer { small }

        NovaText(
            text = tokenUiConfig.symbol,
            style = PolkadotTheme.typography.title.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}
