package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.PermissionContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionContext
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

@HiltViewModel
class PermissionPromptViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val holder: PermissionContextHolder,
    private val router: ProductsRouter,
) : BaseViewModel(), PermissionPromptContract {
    private val requestId = savedStateHandle.get<String>(PermissionPromptBottomSheet.REQUEST_ID)
    private val permissionContext = holder.get()?.takeIf { it.id == requestId }
    override val state: StateFlow<PermissionPromptUiState?> = MutableStateFlow(permissionContext?.toUiState())

    fun onResume() = launchUnit {
        if (permissionContext == null || holder.get() !== permissionContext) router.closePermissionPrompt(requestId)
    }

    override fun onAllowAlwaysClicked() = deliver(PermissionDecision.AllowAlways)

    override fun onAllowOnceClicked() = deliver(PermissionDecision.AllowOnce)

    override fun onDenyClicked() = deliver(PermissionDecision.Deny)

    private fun deliver(decision: PermissionDecision) {
        permissionContext?.deliver(decision)
    }
}

private fun ProductPermissionContext.toUiState(): PermissionPromptUiState {
    val single = permissions.singleOrNull()
    return if (single != null) {
        PermissionPromptUiState.Single(productId.value, single)
    } else {
        PermissionPromptUiState.Batched(
            productId = productId.value,
            permissions = permissions.filterIsInstance<RemotePermission>().toImmutableList(),
        )
    }
}
