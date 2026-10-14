package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.PermissionPromptUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.components.BatchedPromptContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.components.SinglePromptContent
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PermissionPromptScreen(contract: PermissionPromptContract) {
    val state by contract.state.collectAsStateWithLifecycle()
    state?.let {
        PermissionPromptScreenInternal(
            state = it,
            onAllowOnceClicked = contract::onAllowOnceClicked,
            onAllowAlwaysClicked = contract::onAllowAlwaysClicked,
            onDenyClicked = contract::onDenyClicked,
        )
    }
}

@Composable
private fun PermissionPromptScreenInternal(
    state: PermissionPromptUiState,
    onAllowOnceClicked: () -> Unit,
    onAllowAlwaysClicked: () -> Unit,
    onDenyClicked: () -> Unit,
) {
    PromptContainer {
        when (state) {
            is PermissionPromptUiState.Single ->
                SinglePromptContent(state.productId, state.permission)
            is PermissionPromptUiState.Batched ->
                BatchedPromptContent(state.productId, state.permissions)
        }

        VerticalSpacer { extraLarge }

        PromptActions(
            allowOnceRes = state.allowOnceRes,
            allowAlwaysRes = state.allowAlwaysRes,
            denyRes = state.denyRes,
            onAllowOnceClicked = onAllowOnceClicked,
            onAllowAlwaysClicked = onAllowAlwaysClicked,
            onDenyClicked = onDenyClicked,
        )
    }
}

@Composable
private fun PromptContainer(content: @Composable ColumnScope.() -> Unit) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(
                    top = PolkadotTheme.spacings.large,
                    bottom = PolkadotTheme.spacings.mediumIncreased,
                    start = PolkadotTheme.spacings.mediumIncreased,
                    end = PolkadotTheme.spacings.mediumIncreased,
                ),
            horizontalAlignment = Alignment.CenterHorizontally,
            content = content,
        )
    }
}

@Composable
private fun PromptActions(
    @StringRes allowOnceRes: Int,
    @StringRes allowAlwaysRes: Int,
    @StringRes denyRes: Int,
    onAllowOnceClicked: () -> Unit,
    onAllowAlwaysClicked: () -> Unit,
    onDenyClicked: () -> Unit,
) {
    PolkadotTextButton(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(allowOnceRes),
        style = PolkadotButtonStyle.secondary(),
        onClick = onAllowOnceClicked,
    )

    VerticalSpacer { small }

    PolkadotTextButton(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(allowAlwaysRes),
        style = PolkadotButtonStyle.secondary(),
        onClick = onAllowAlwaysClicked,
    )

    VerticalSpacer { small }

    PolkadotTextButton(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(denyRes),
        style = PolkadotButtonStyle.ghost(),
        onClick = onDenyClicked,
    )
}

@get:StringRes
private val PermissionPromptUiState.allowOnceRes: Int
    get() = when (this) {
        is PermissionPromptUiState.Single -> RCommon.string.product_permission_allow_once
        is PermissionPromptUiState.Batched -> RCommon.string.product_permission_allow_once_all
    }

@get:StringRes
private val PermissionPromptUiState.allowAlwaysRes: Int
    get() = when (this) {
        is PermissionPromptUiState.Single -> RCommon.string.product_permission_allow_always
        is PermissionPromptUiState.Batched -> RCommon.string.product_permission_allow_always_all
    }

@get:StringRes
private val PermissionPromptUiState.denyRes: Int
    get() = when (this) {
        is PermissionPromptUiState.Single -> RCommon.string.product_permission_deny
        is PermissionPromptUiState.Batched -> RCommon.string.product_permission_deny_all
    }

@Preview
@Composable
private fun PermissionPromptScreenPreview() {
    PolkadotTheme {
        PermissionPromptScreenInternal(
            state = PermissionPromptUiState.Single(
                productId = "alice.dot",
                permission = ProductPermission.RemotePermission.NetworkAccess(domain = "example.com"),
            ),
            onAllowOnceClicked = {},
            onAllowAlwaysClicked = {},
            onDenyClicked = {},
        )
    }
}
