package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import kotlinx.coroutines.flow.StateFlow

interface TransactionSignContract {
    val state: StateFlow<LoadingState<TransactionSignUiState>>

    fun onApproveClicked()

    fun onRejectClicked()

    fun onDetailsClicked()

    fun onBackFromDetailsClicked()
}

sealed interface SigningContent {
    class Transaction(val callName: String, val detailsJson: String) : SigningContent
    class RawMessage(val hexData: String) : SigningContent
    class VrfTranscript(val transcriptLabel: String, val itemsText: String) : SigningContent
}

sealed interface SigningAccountUi {
    data class Product(val productId: String, val derivationIndex: String) : SigningAccountUi
    data object IdentityAccount : SigningAccountUi
    data class Legacy(val address: String) : SigningAccountUi
}

data class TransactionSignUiState(
    val requesterName: String,
    val requesterIconUrl: String,
    val content: SigningContent,
    val signingAccount: SigningAccountUi,
    val signing: Boolean = false,
    val showingDetails: Boolean = false,
)
