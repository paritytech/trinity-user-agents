package io.paritytech.polkadotapp.feature_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.NativeFeePayment
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

class TransferArguments(
    val origin: TransactionOrigin,
    val recipient: AccountId,
    val amount: Balance,
    val feePayment: FeePayment = NativeFeePayment(),
)
