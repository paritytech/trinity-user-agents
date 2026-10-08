package io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.asUtf8OrHex
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.asDisplayString
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContext
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContextHolder
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class CrossProductProofViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val context: CrossProductProofContext,
    private val holder: CrossProductProofContextHolder,
) : BaseViewModel() {
    val state: StateFlow<CrossProductProofUiState>
        field = MutableStateFlow(
            CrossProductProofUiState(
                callingProduct = context.callingProduct.value,
                onBehalfOf = context.onBehalfOf.value,
                suffix = context.suffix.asDisplayString(),
                message = context.message.asUtf8OrHex(),
            )
        )

    // Set once this sheet has nothing left to do; one that was not on top then closes when it is next shown
    private var finished = false

    private val withdrawalWatch = launch {
        context.awaitWithdrawal()
        close()
    }

    fun onResume() = launchUnit {
        if (finished) close()
    }

    fun onApproveClicked() = launchUnit {
        withdrawalWatch.cancel()
        context.deliverApproved()
        close()
    }

    fun onRejectClicked() = launchUnit {
        withdrawalWatch.cancel()
        context.deliverRejected()
        close()
    }

    private suspend fun close() {
        finished = true
        router.closeCrossProductProofPrompt(context.id)
    }

    override fun onCleared() {
        super.onCleared()
        // A sheet that goes without an answer must not leave its caller waiting; a no-op once answered
        context.deliverRejected()
        holder.clear(context)
    }
}

data class CrossProductProofUiState(
    val callingProduct: String,
    val onBehalfOf: String,
    val suffix: String,
    val message: String,
)
