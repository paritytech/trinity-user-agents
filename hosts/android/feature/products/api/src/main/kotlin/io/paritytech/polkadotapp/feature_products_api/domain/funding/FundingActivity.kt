package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import kotlin.time.Instant

/** How value moves between the user's balance and the outside. */
enum class FundingRailType {
    CARD,
    BANK,
    CRYPTO,
}

/** The CASH card's funding lists: sessions in flight, then ended ones, newest first. */
data class FundingActivity(
    val inFlight: List<FundingActivityItem>,
    val history: List<FundingActivityItem>,
)

/** One row of the funding lists. */
data class FundingActivityItem(
    val id: String,
    val direction: FundingDirection,
    val rail: FundingRailType?,
    val amount: Balance?,
    val date: Instant,
    val status: FundingActivityStatus,
)

sealed interface FundingActivityStatus {
    /** In flight, with where it has got to; [delayed] when slower than the provider said. */
    data class InProgress(val step: FundingActivityStep, val delayed: Boolean) : FundingActivityStatus

    data object ToppedUp : FundingActivityStatus

    data object Sent : FundingActivityStatus

    data object Refunded : FundingActivityStatus

    data object PayoutFailed : FundingActivityStatus

    data class Failed(val reason: FundingFailureReason) : FundingActivityStatus
}

/** Where a session in flight stands, as one line under it. */
sealed interface FundingActivityStep {
    /** Value in, before the provider has the user's card payment. */
    data object WaitingForPayment : FundingActivityStep

    /** Value in, before the provider has the user's bank transfer. */
    data object WaitingForTransfer : FundingActivityStep

    /** Value in, before the provider has the user's crypto deposit. */
    data object WaitingForDeposit : FundingActivityStep

    /** Value in, once the provider has the payment: converting it to the balance asset. */
    data object Converting : FundingActivityStep

    /** Value in, the provider retrying a top-up that claimed only part of the amount. */
    data object Retrying : FundingActivityStep

    /** Value out, until the balance has been debited. */
    data object TransactionInitiated : FundingActivityStep

    /** Value out to a card or a bank, debited and waiting for the provider's payout. */
    data object PreparingPayout : FundingActivityStep

    /** Value out to a crypto wallet, debited and converting to [asset]. */
    data class ConvertingOut(val asset: String) : FundingActivityStep

    /** Value out, the provider retrying the payout. */
    data object RetryingPayout : FundingActivityStep
}
