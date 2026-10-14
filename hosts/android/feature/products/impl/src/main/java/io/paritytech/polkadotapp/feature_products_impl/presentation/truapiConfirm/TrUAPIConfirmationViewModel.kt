package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIConfirmationContext
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIConfirmationContextHolder
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class TrUAPIConfirmationViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val context: TrUAPIConfirmationContext,
    private val holder: TrUAPIConfirmationContextHolder,
) : BaseViewModel(), TrUAPIConfirmationContract {
    private val deciding = MutableStateFlow(false)

    override val state: StateFlow<LoadingState<TrUAPIConfirmationUiState>> =
        deciding.map { context.confirmation.toUiState() }
            .withLoading("TrUAPIConfirmation")
            .inBackground()
            .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onApproveClicked() = decide { context.approve() }

    override fun onRejectClicked() = decide { context.reject() }

    private fun decide(answer: () -> Unit) = launchUnit {
        if (deciding.value) return@launchUnit
        deciding.value = true
        answer()
        router.back()
    }

    override fun onCleared() {
        // The core is still blocked if the sheet went away without an answer,
        // so a dismissal has to resolve as a refusal.
        context.reject()
        holder.clear(context)
        super.onCleared()
    }
}
