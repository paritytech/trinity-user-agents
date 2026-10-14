package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.common.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction

@Composable
fun MnemonicBase(
    modifier: Modifier,
    onBackAction: () -> Unit,
    title: String,
    description: String,
    content: @Composable ColumnScope.() -> Unit
) {
    Column(
        modifier = modifier
    ) {
        PolkadotTopBar(
            navigationAction = rememberTopBarAction(onBackAction),
            titleAlignment = TopBarTitleAlignment.Center,
        )

        VerticalSpacer { small }

        MnemonicHeader(
            title = title,
            description = description,
        )

        VerticalSpacer { 48.dp }

        content()
    }
}
