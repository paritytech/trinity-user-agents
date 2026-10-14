package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import java.util.concurrent.TimeoutException

sealed class SendError(message: String) : Throwable(message) {
    data object SettlementTimeout : SendError("coins settlement was not confirmed in time")

    data object NotEnoughFunds : SendError("transfer amount exceeds the available balance")

    data object BalanceUnavailable : SendError("available balance could not be fetched")

    class Unknown(override val cause: Throwable) : SendError("failed to send coins")
}

fun Throwable.asSendError(): SendError = when (this) {
    is SendError -> this
    is TimeoutException -> SendError.SettlementTimeout

    else -> SendError.Unknown(this)
}
