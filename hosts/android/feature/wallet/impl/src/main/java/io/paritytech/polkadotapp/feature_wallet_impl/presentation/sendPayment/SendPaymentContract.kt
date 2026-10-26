package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

interface SendPaymentContract {
    val state: StateFlow<SendPaymentUiState>

    val messageEvents: Flow<Int>

    fun onInputChange(value: String)
    fun onRecipientSelect(recipient: PaymentSearchResultUiModel)

    fun onSendToYourselfClick()

    fun onSendMoneyOutClick()

    fun onFundingHistoryClick()

    fun onFundingSheetBack()

    fun onFundingSheetDismiss()

    fun onFundingRowClick(id: String)

    fun onPasteClick()

    fun onScannerClick()

    fun onBackClick()
}
