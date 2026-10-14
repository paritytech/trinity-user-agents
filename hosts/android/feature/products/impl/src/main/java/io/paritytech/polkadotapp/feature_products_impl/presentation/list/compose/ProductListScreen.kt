package io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.loading.onLoading
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.ProductListViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose.components.EmptyState
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.compose.components.ProductItem
import io.paritytech.polkadotapp.feature_products_impl.presentation.list.models.ProductListItemUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ProductListScreen(viewModel: ProductListViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ProductListScreenInternal(
        state = state,
        onProductSelected = viewModel::onProductSelected,
        onBack = viewModel::onBack
    )
}

@Composable
private fun ProductListScreenInternal(
    state: LoadingState<List<ProductListItemUiModel>>,
    onProductSelected: (ProductListItemUiModel) -> Unit,
    onBack: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.products_toolbar_title),
                navigationAction = rememberTopBarAction(action = onBack),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { extraMedium }

            state
                .onLoaded { items ->
                    if (items.isEmpty()) {
                        EmptyState()
                    } else {
                        LazyColumn(
                            modifier = Modifier.fillMaxSize(),
                            contentPadding = WindowInsets.navigationBars.asPaddingValues()
                        ) {
                            items(items) {
                                ProductItem(
                                    item = it,
                                    onClick = { onProductSelected(it) }
                                )
                            }
                        }
                    }
                }
                .onLoading {
                    LoadingScreenState()
                }
        }
    }
}

@Preview
@Composable
private fun ProductListScreenPreview() {
    PolkadotTheme {
        ProductListScreenInternal(
            state = LoadingState.Loaded(
                listOf(
                    ProductListItemUiModel(
                        id = ProductId.fromStoredValue("1"),
                        name = "Web3 Summit",
                        iconUrl = null,
                    ),
                    ProductListItemUiModel(
                        id = ProductId.fromStoredValue("2"),
                        name = "Polkadot Decoded",
                        iconUrl = null,
                    ),
                    ProductListItemUiModel(
                        id = ProductId.fromStoredValue("3"),
                        name = "Sub0 Conference",
                        iconUrl = null,
                    )
                )
            ),
            onProductSelected = {},
            onBack = {}
        )
    }
}
