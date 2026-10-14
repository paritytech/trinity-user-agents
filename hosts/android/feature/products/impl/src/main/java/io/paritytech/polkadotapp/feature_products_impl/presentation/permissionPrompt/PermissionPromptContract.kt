package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

interface PermissionPromptContract {
    val state: StateFlow<PermissionPromptUiState?>

    fun onAllowAlwaysClicked()

    fun onAllowOnceClicked()

    fun onDenyClicked()
}

@Immutable
sealed interface PermissionPromptUiState {
    data class Single(
        val productId: String,
        val permission: ProductPermission,
    ) : PermissionPromptUiState

    data class Batched(
        val productId: String,
        val permissions: ImmutableList<RemotePermission>,
    ) : PermissionPromptUiState
}
