package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.OneShotEventChannel
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.feature_account_api.presentation.address.converter.ParseAddressConverterFactory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.toParcel
import io.paritytech.polkadotapp.feature_chats_api.domain.error.asStartChatError
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasEstablishedChat
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_api.presentation.ChatStarter
import io.paritytech.polkadotapp.feature_chats_api.presentation.address.ContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_chats_api.presentation.error.toPresentationError
import io.paritytech.polkadotapp.feature_transfers_api.presentation.PreviousPaymentsAddressConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.ParseAddressUsernameConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.UsernameAddressConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.filterAvailableUsernameSymbols
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.WithdrawUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.ScanAddressQrResultPayload
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain.SendPaymentInteractor
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class SendPaymentViewModel @Inject constructor(
    private val walletRouter: PocketRouter,
    private val clipboardService: ClipboardService,
    private val getContactsUseCase: GetContactsUseCase,
    private val chatStarter: ChatStarter,
    addressInputMixinFactory: AddressInputMixin.Factory,
    usernameAddressConverterFactory: UsernameAddressConverterFactory,
    parseAddressConverterFactory: ParseAddressConverterFactory,
    parserAddressUsernameConverterFactory: ParseAddressUsernameConverterFactory,
    previousPaymentsAddressConverterFactory: PreviousPaymentsAddressConverterFactory,
    contactsAddressConverterFactory: ContactsAddressConverterFactory,
    private val interactor: SendPaymentInteractor,
) : BaseViewModel(), SendPaymentContract {
    private val contacts = flowOf { getContactsUseCase() }
        .shareInBackground()

    private val fundingSheetInProgress = MutableStateFlow(false)

    private val _messageEvents = OneShotEventChannel<Int>()
    override val messageEvents = _messageEvents.receiveAsFlow()

    private val addressInputMixin = addressInputMixinFactory.create(
        coroutineScope = viewModelScope,
        localConverters = listOf(
            previousPaymentsAddressConverterFactory.create(interactor.chainId()),
            contactsAddressConverterFactory.create(),
        ),
        remoteConverters = listOf(
            parserAddressUsernameConverterFactory.create(
                parseAddressConverterFactory.create(interactor.chainId())
            ),
            usernameAddressConverterFactory.create(),
        ),
    )

    private val searchResults = combine(addressInputMixin.addressCandidates, contacts) { candidates, contacts ->
        val blockedAccountIds = contacts.filter { it.isBlocked }.map { it.accountId }.toSet()

        candidates.toSearchResults(hiddenAccountIds = blockedAccountIds)
    }

    override val state: StateFlow<SendPaymentUiState> = combine(
        addressInputMixin.input,
        searchResults
    ) { inputValue, results ->
        SendPaymentUiState(
            input = inputValue,
            sendToYourselfVisible = inputValue.isEmpty(),
            results = results,
        )
    }.stateIn(
        scope = this,
        started = SharingStarted.Eagerly,
        initialValue = SendPaymentUiState(
            input = "",
            sendToYourselfVisible = true,
            results = PaymentSearchResults.Loading
        )
    )

    fun onQrResult(payload: ScanAddressQrResultPayload) {
        onInputChange(payload.address)
    }

    override fun onInputChange(value: String) {
        addressInputMixin.input.update { value.filterAvailableUsernameSymbols() }
    }

    override fun onRecipientSelect(recipient: PaymentSearchResultUiModel) = launchUnit {
        val accountId = recipient.extractedAddress.accountId
        val contact = contacts.first().find { it.accountId == accountId }

        if (contact != null && contact.hasEstablishedChat()) {
            walletRouter.openSendEnterAmount(
                SendEnterAmountPayload(
                    showTransactionResult = true,
                    transferMethod = TransferMethodPayload.CoinsViaChat(recipient.extractedAddress.toParcel()),
                    amountPreset = null,
                )
            )
        } else {
            _messageEvents.trySend(RCommon.string.send_payment_open_chat_message)

            chatStarter.openChatWith(accountId)
                .onFailure { showPresentationError(it.asStartChatError().toPresentationError()) }
        }
    }

    override fun onSendToYourselfClick() = launchUnit {
        if (fundingSheetInProgress.value) return@launchUnit
        fundingSheetInProgress.enable()
        interactor.getFundingConfig()
            .onSuccess { walletRouter.openSpaSheet(it.offrampUrl) }
            .onFailure { showPresentationError(WithdrawUnavailablePresentationError(it)) }
        fundingSheetInProgress.disable()
    }

    override fun onPasteClick() {
        clipboardService.getPrimaryClip()?.let { onInputChange(it) }
    }

    override fun onScannerClick() {
        walletRouter.openScanAddressQr()
    }

    override fun onBackClick() {
        walletRouter.back()
    }
}
