package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.toSubstrateAddress
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.SigningFailedPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.asDisplayString
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ParsedSigningContent
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.TransactionSignInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import timber.log.Timber
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class TransactionSignViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val interactor: TransactionSignInteractor,
    private val signingContextHolder: SigningContextHolder,
    private val signingContext: SigningContext,
) : BaseViewModel(), TransactionSignContract {
    private val signing = MutableStateFlow(false)
    private val showingDetails = MutableStateFlow(false)

    private val parsedSigningContentFlow = flowOf {
        interactor.parseSigningContent()
    }

    private val humanReadableFlow = flowOf {
        interactor.humanReadableRepresentation()
    }

    override val state: StateFlow<LoadingState<TransactionSignUiState>> = combine(
        parsedSigningContentFlow,
        humanReadableFlow,
        signing,
        showingDetails
    ) { parsedResult, humanReadableResult, isSigning, isShowingDetails ->
        combineResults(parsedResult, humanReadableResult) { parsed, humanReadable ->
            TransactionSignUiState(
                requesterName = signingContext.requesterName,
                requesterIconUrl = signingContext.requesterIconUrl,
                content = parsed.toSigningContent(humanReadable),
                signingAccount = interactor.account.toUi(),
                signing = isSigning,
                showingDetails = isShowingDetails,
            )
        }
    }
        .withLoading("TransactionSign")
        .inBackground()
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onApproveClicked() = launchUnit {
        if (signing.value) return@launchUnit

        signing.value = true

        Timber.d("Approve clicked for ${signingContext.requesterName}")

        signingContext.approve { interactor.sign() }
            .onSuccess {
                showMessage(RCommon.string.sign_transaction_signed)

                router.back()
            }
            .onFailure { showPresentationError(SigningFailedPresentationError(it)) }

        signing.value = false
    }

    override fun onRejectClicked() = launchUnit {
        signing.value = true

        Timber.d("Reject clicked for ${signingContext.requesterName}")

        signingContext.deliverRejection()
            .onFailure { showPresentationError(UnexpectedPresentationError(it)) }

        router.back()

        signing.value = false
    }

    override fun onDetailsClicked() {
        showingDetails.value = true
    }

    override fun onBackFromDetailsClicked() {
        showingDetails.value = false
    }

    override fun onCleared() {
        super.onCleared()

        signingContext.onAbandoned()
        signingContextHolder.clear(signingContext)
    }

    private fun SigningAccount.toUi(): SigningAccountUi {
        return when (this) {
            is SigningAccount.Product -> SigningAccountUi.Product(
                productId = accountId.productId,
                derivationIndex = accountId.index.asDisplayString(),
            )

            SigningAccount.IdentityAccount -> SigningAccountUi.IdentityAccount

            is SigningAccount.Legacy -> SigningAccountUi.Legacy(accountId.toSubstrateAddress(GENERIC_SS58_PREFIX))
        }
    }

    private companion object {
        // The sheet has no chain for a legacy account, so it shows the
        // network-agnostic form rather than implying one.
        const val GENERIC_SS58_PREFIX: Short = 42
    }

    private fun ParsedSigningContent.toSigningContent(humanReadable: String): SigningContent {
        return when (this) {
            is ParsedSigningContent.Transaction -> SigningContent.Transaction(
                callName = "${call.module.name}.${call.function.name}",
                detailsJson = humanReadable
            )

            is ParsedSigningContent.Raw -> SigningContent.RawMessage(
                hexData = humanReadable
            )

            is ParsedSigningContent.Vrf -> SigningContent.VrfTranscript(
                transcriptLabel = transcriptLabel,
                itemsText = humanReadable
            )
        }
    }
}
