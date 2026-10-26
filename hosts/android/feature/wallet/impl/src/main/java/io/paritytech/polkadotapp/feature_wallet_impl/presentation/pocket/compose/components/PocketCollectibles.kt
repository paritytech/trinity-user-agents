package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCollectiblesImageSharedElement
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PocketCollectibles(
    onBack: () -> Unit,
    onViewButtonClick: () -> Unit
) {
    BackHandler { onBack() }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .systemBarsPadding()
    ) {
        PolkadotTopBar(
            title = stringResource(RCommon.string.pocket_collectibles_title),
            navigationAction = rememberTopBarAction(onBack),
            titleAlignment = TopBarTitleAlignment.Center
        )

        CollectiblesSketchbook(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased)
                .pocketCollectiblesImageSharedElement(),
            onViewButtonClick = onViewButtonClick,
            expanded = true
        )
    }
}
