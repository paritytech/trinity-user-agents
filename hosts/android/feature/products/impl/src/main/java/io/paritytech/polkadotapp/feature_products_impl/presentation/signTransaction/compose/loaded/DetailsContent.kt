package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose.loaded

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun DetailsContent(
    detailsText: String,
    title: String,
    onBackClicked: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
    ) {
        PolkadotTopBar(
            title = title,
            navigationAction = rememberTopBarAction(
                action = onBackClicked,
                icon = NovaIcons.ArrowLeft
            ),
            titleAlignment = TopBarTitleAlignment.Center,
        )

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 600.dp)
                .verticalScroll(rememberScrollState())
                .padding(PolkadotTheme.spacings.mediumIncreased)
        ) {
            PolkadotSurface(
                modifier = Modifier.fillMaxWidth(),
                color = PolkadotTheme.colors.bg.surface.container,
                shape = PolkadotTheme.shapes.medium
            ) {
                NovaText(
                    modifier = Modifier.padding(PolkadotTheme.spacings.extraMedium),
                    text = detailsText,
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary
                )
            }
        }
    }
}
