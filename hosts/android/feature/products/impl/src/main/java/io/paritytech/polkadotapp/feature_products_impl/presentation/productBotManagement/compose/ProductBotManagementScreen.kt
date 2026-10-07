package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.compose

import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.compound.NovaSwitch
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Edit
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductBotManagementContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductBotManagementState
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductDialogState
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ProductBotManagementScreen(contract: ProductBotManagementContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    ProductBotManagementScreenInternal(
        state = state,
        onBackClick = contract::onBackClick,
        onAddProductClick = contract::onAddProductClick,
        onProductClick = contract::onProductClick,
        onEditProductClick = contract::onEditProductClick,
        onDeleteProductClick = contract::onDeleteProductClick,
        onDialogDismiss = contract::onDialogDismiss,
        onNameChanged = contract::onDotNsDomainChanged,
        onWorkerUrlChanged = contract::onWorkerUrlChanged,
        onCardIdChanged = contract::onCardIdChanged,
        onCardTitleChanged = contract::onCardTitleChanged,
        onPreviewUrlChanged = contract::onPreviewUrlChanged,
        onRunsOnWasmiChanged = contract::onRunsOnWasmiChanged,
        onDialogConfirm = contract::onDialogConfirm,
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProductBotManagementScreenInternal(
    state: ProductBotManagementState,
    onBackClick: () -> Unit,
    onAddProductClick: () -> Unit,
    onProductClick: (ProductId) -> Unit,
    onEditProductClick: (String) -> Unit,
    onDeleteProductClick: (String) -> Unit,
    onDialogDismiss: () -> Unit,
    onNameChanged: (String) -> Unit,
    onWorkerUrlChanged: (String) -> Unit,
    onCardIdChanged: (String) -> Unit,
    onCardTitleChanged: (String) -> Unit,
    onPreviewUrlChanged: (String) -> Unit,
    onRunsOnWasmiChanged: (Boolean) -> Unit,
    onDialogConfirm: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.product_bot_management_title),
                navigationAction = rememberTopBarAction(
                    action = onBackClick,
                    icon = NovaIcons.Close
                ),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { mediumIncreased }

            PolkadotTextButton(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.large),
                text = stringResource(RCommon.string.product_bot_management_add),
                onClick = onAddProductClick,
            )

            VerticalSpacer { mediumIncreased }

            if (state.products.isEmpty()) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(PolkadotTheme.spacings.large),
                    contentAlignment = Alignment.Center
                ) {
                    NovaText(
                        text = stringResource(RCommon.string.product_bot_management_empty),
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.tertiary
                    )
                }
            } else {
                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    items(state.products, key = { it.id.value }) { product ->
                        ProductItem(
                            product = product,
                            onClick = { onProductClick(product.id) },
                            onEditClick = { onEditProductClick(product.id.value) },
                            onDeleteClick = { onDeleteProductClick(product.id.value) },
                        )
                    }
                }
            }
        }
    }

    val dialogState = state.dialogState
    if (dialogState is ProductDialogState.Form) {
        ProductFormDialog(
            state = dialogState,
            tldSuffix = state.tldSuffix,
            onDismiss = onDialogDismiss,
            onNameChanged = onNameChanged,
            onWorkerUrlChanged = onWorkerUrlChanged,
            onCardIdChanged = onCardIdChanged,
            onCardTitleChanged = onCardTitleChanged,
            onPreviewUrlChanged = onPreviewUrlChanged,
            onRunsOnWasmiChanged = onRunsOnWasmiChanged,
            onConfirm = onDialogConfirm,
        )
    }
}

@Composable
private fun ProductItem(
    product: ProductUiModel,
    onClick: () -> Unit,
    onEditClick: () -> Unit,
    onDeleteClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.extraMedium,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = product.id.value,
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
            )
        }

        HorizontalSpacer { small }

        IconButton(onClick = onEditClick) {
            Icon(
                imageVector = NovaIcons.Edit,
                contentDescription = null,
                tint = PolkadotTheme.colors.fg.secondary,
            )
        }

        IconButton(onClick = onDeleteClick) {
            Icon(
                imageVector = NovaIcons.Close,
                contentDescription = null,
                tint = PolkadotTheme.colors.fg.secondary,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProductFormDialog(
    state: ProductDialogState.Form,
    tldSuffix: String,
    onDismiss: () -> Unit,
    onNameChanged: (String) -> Unit,
    onWorkerUrlChanged: (String) -> Unit,
    onCardIdChanged: (String) -> Unit,
    onCardTitleChanged: (String) -> Unit,
    onPreviewUrlChanged: (String) -> Unit,
    onRunsOnWasmiChanged: (Boolean) -> Unit,
    onConfirm: () -> Unit,
) {
    val isEditing = state.productId != null
    val title = if (isEditing) {
        stringResource(RCommon.string.product_bot_management_edit_title)
    } else {
        stringResource(RCommon.string.product_bot_management_add_title)
    }

    BasicAlertDialog(onDismissRequest = onDismiss) {
        PolkadotSurface(
            color = PolkadotTheme.colors.bg.surface.container,
            shape = RoundedCornerShape(28.dp),
            shadowElevation = 5.dp
        ) {
            Column(
                modifier = Modifier
                    .verticalScroll(rememberScrollState())
                    .padding(PolkadotTheme.spacings.large),
            ) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.headline.small,
                    color = PolkadotTheme.colors.fg.primary
                )

                VerticalSpacer { mediumIncreased }

                NovaTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = state.dotNsName,
                    onValueChange = onNameChanged,
                    placeholder = {
                        NovaText(
                            text = stringResource(RCommon.string.product_bot_management_name_hint, tldSuffix),
                            style = PolkadotTheme.typography.body.large,
                            color = PolkadotTheme.colors.fg.tertiary
                        )
                    }
                )

                VerticalSpacer { extraMedium }

                NovaTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = state.workerUrl,
                    onValueChange = onWorkerUrlChanged,
                    placeholder = {
                        NovaText(
                            text = stringResource(RCommon.string.product_bot_management_url_hint),
                            style = PolkadotTheme.typography.body.large,
                            color = PolkadotTheme.colors.fg.tertiary
                        )
                    }
                )

                VerticalSpacer { extraMedium }

                DebugFormField(
                    value = state.cardId,
                    onValueChange = onCardIdChanged,
                    hint = RCommon.string.product_bot_management_card_id_hint,
                )

                VerticalSpacer { extraMedium }

                DebugFormField(
                    value = state.cardTitle,
                    onValueChange = onCardTitleChanged,
                    hint = RCommon.string.product_bot_management_card_title_hint,
                )

                VerticalSpacer { extraMedium }

                DebugFormField(
                    value = state.previewUrl,
                    onValueChange = onPreviewUrlChanged,
                    hint = RCommon.string.product_bot_management_preview_url_hint,
                )

                VerticalSpacer { extraMedium }

                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onRunsOnWasmiChanged(!state.runsOnWasmi) },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    NovaText(
                        modifier = Modifier.weight(1f),
                        text = stringResource(RCommon.string.product_bot_management_wasmi_worker),
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.primary,
                    )

                    NovaSwitch(
                        checked = state.runsOnWasmi,
                        onCheckedChange = onRunsOnWasmiChanged,
                    )
                }

                VerticalSpacer { large }

                PolkadotTextButton(
                    modifier = Modifier.align(Alignment.End),
                    text = stringResource(RCommon.string.product_bot_management_confirm),
                    onClick = onConfirm,
                    enabled = state.workerUrl.isNotBlank() &&
                        state.dotNsName.isNotBlank() &&
                        !state.isSubmitting,
                    loading = state.isSubmitting
                )
            }
        }
    }
}

@Composable
private fun DebugFormField(
    value: String,
    onValueChange: (String) -> Unit,
    @StringRes hint: Int,
) {
    NovaTextField(
        modifier = Modifier.fillMaxWidth(),
        value = value,
        onValueChange = onValueChange,
        placeholder = {
            NovaText(
                text = stringResource(hint),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.tertiary
            )
        }
    )
}

@Preview
@Composable
private fun ProductBotManagementScreenPreview() {
    PolkadotTheme {
        ProductBotManagementScreenInternal(
            state = ProductBotManagementState(
                products = listOf(
                    ProductUiModel(id = ProductId.fromStoredValue("1.dot"), name = "Test Product", appUrl = "https://1.dot"),
                    ProductUiModel(id = ProductId.fromStoredValue("2.dot"), name = "Another Product", appUrl = "https://2.dot"),
                ),
            ),
            onBackClick = {},
            onAddProductClick = {},
            onProductClick = {},
            onEditProductClick = {},
            onDeleteProductClick = {},
            onDialogDismiss = {},
            onNameChanged = {},
            onWorkerUrlChanged = {},
            onCardIdChanged = {},
            onCardTitleChanged = {},
            onPreviewUrlChanged = {},
            onRunsOnWasmiChanged = {},
            onDialogConfirm = {},
        )
    }
}
