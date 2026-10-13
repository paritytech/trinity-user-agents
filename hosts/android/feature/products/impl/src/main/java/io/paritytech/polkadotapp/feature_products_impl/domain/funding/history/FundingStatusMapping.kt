package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingStatus

/** The CASH card's line for the status the core gives a session in [direction]. */
internal fun FundingStatus.toActivityStatus(direction: FundingDirection): FundingActivityStatus = when (this) {
    FundingStatus.WaitingForPayment -> inProgress(FundingActivityStep.WaitingForPayment)
    FundingStatus.WaitingForTransfer -> inProgress(FundingActivityStep.WaitingForTransfer)
    FundingStatus.WaitingForDeposit -> inProgress(FundingActivityStep.WaitingForDeposit)
    FundingStatus.ConvertingToCash -> inProgress(FundingActivityStep.Converting)
    FundingStatus.TransactionInitiated -> inProgress(FundingActivityStep.TransactionInitiated)
    is FundingStatus.ConvertingTo -> inProgress(FundingActivityStep.ConvertingOut(asset))
    FundingStatus.PreparingPayout -> inProgress(FundingActivityStep.PreparingPayout)
    FundingStatus.Retrying -> inProgress(
        if (direction == FundingDirection.OUT) FundingActivityStep.RetryingPayout else FundingActivityStep.Retrying,
    )

    FundingStatus.TakingLonger -> inProgress(FundingActivityStep.TakingLonger)
    FundingStatus.ToppedUp -> FundingActivityStatus.ToppedUp
    FundingStatus.Sent -> FundingActivityStatus.Sent
    FundingStatus.Refunded -> FundingActivityStatus.Refunded
    is FundingStatus.PayoutFailed -> FundingActivityStatus.PayoutFailed(refunded?.toBalance())
    is FundingStatus.Failed -> FundingActivityStatus.Failed(reason.toReason())
}

private fun inProgress(step: FundingActivityStep) = FundingActivityStatus.InProgress(step)
