package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.text.style.TextOverflow
import io.paritytech.polkadotapp.app.root.presentation.main.compose.title
import io.paritytech.polkadotapp.app.root.presentation.root.StatusBarTitleModel
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.ProductStatusBarTitle

@Composable
fun StatusBarTitle(model: StatusBarTitleModel) {
    when (model) {
        is StatusBarTitleModel.Tab -> NovaText(
            text = model.tab.title(),
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )

        StatusBarTitleModel.Product -> ProductStatusBarTitle()

        StatusBarTitleModel.None -> Unit
    }
}
