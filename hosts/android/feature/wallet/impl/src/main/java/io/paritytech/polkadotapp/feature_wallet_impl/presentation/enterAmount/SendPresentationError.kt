package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.SendError
import io.paritytech.polkadotapp.common.R as RCommon

fun SendError.toPresentationError(): PresentationThrowable = when (this) {
    SendError.SettlementTimeout -> SettlementTimeoutPresentationError(this)

    SendError.NotEnoughFunds -> NotEnoughFundsPresentationError(this)

    SendError.BalanceUnavailable -> BalanceUnavailablePresentationError(this)

    is SendError.Unknown -> UnexpectedPresentationError(this)
}

class SettlementTimeoutPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.send_error_settlement_timeout)

class NotEnoughFundsPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.send_error_not_enough_funds)

class BalanceUnavailablePresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.send_error_balance_unavailable)
