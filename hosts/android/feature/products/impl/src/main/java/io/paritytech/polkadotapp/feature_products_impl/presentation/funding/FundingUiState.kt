package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingReasonText
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import kotlinx.collections.immutable.ImmutableList
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import kotlin.time.Instant

/** The screens of the funding sheet, the amount screen at the root. */
enum class FundingScreen {
    AMOUNT,
    SUMMARY,
    FEES,
    COUNTRY,
    PROVIDERS,
    NETWORK,
    TOKEN,
    DEPOSIT,
    CANCEL_CONFIRM,
    CANCEL_OFFER,
}

data class FundingSheetUiState(
    val screen: FundingScreen,
    val amount: FundingAmountUiState,
    val summary: FundingSummaryUiState,
    val fees: FundingFeesUiState?,
    val country: FundingCountryUiState,
    val providers: FundingProvidersUiState,
    val networks: ImmutableList<FundingNetworkRow>,
    val tokens: FundingTokensUiState,
    val deposit: FundingDepositUiState,
)

data class FundingAmountUiState(
    val isWithdraw: Boolean,
    val rails: ImmutableList<FundingRailTab>,
    val amount: String,
    val digitCount: Int,
    val hasAmount: Boolean,
    val available: String?,
    val note: FundingAmountNote?,
    val presets: ImmutableList<FundingPreset>,
    val canContinue: Boolean,
)

data class FundingRailTab(
    val rail: FundingRail,
    val selected: Boolean,
    val enabled: Boolean,
)

data class FundingPreset(
    val value: BigDecimal,
    val label: String,
)

/** The line under the amount: a learned minimum, or why the amount cannot go ahead. */
sealed interface FundingAmountNote {
    val isError: Boolean

    data class Minimum(val amount: String, val withdraw: Boolean) : FundingAmountNote {
        override val isError = false
    }

    data class BelowMinimum(val amount: String) : FundingAmountNote {
        override val isError = true
    }

    data class AboveMaximum(val amount: String) : FundingAmountNote {
        override val isError = true
    }

    data class NotEnough(val symbol: String) : FundingAmountNote {
        override val isError = true
    }
}

data class FundingSummaryUiState(
    val isWithdraw: Boolean,
    val rail: FundingRail,
    val headline: String?,
    val isQuoting: Boolean,
    val showCountry: Boolean,
    val country: FundingCountryUi?,
    val provider: FundingProviderBrand?,
    val payout: String?,
    val eta: FundingEta?,
    val bankRateNoteSymbol: String?,
    val problem: FundingSummaryProblem?,
    val canStart: Boolean,
    val isStarting: Boolean,
)

/** When the provider says the funds arrive, rounded up. */
sealed interface FundingEta {
    data class Minutes(val count: Int) : FundingEta

    data class Hours(val count: Int) : FundingEta

    data class Days(val count: Int) : FundingEta
}

sealed interface FundingSummaryProblem {
    data object StartFailed : FundingSummaryProblem

    data object NoProvider : FundingSummaryProblem

    /** Every provider gave the same reason for not quoting. */
    data class QuoteRefused(val text: FundingReasonText) : FundingSummaryProblem

    /** The session ended without moving value. */
    data class Failed(val reason: FundingFailureReason) : FundingSummaryProblem

    data class Minimum(val amount: String) : FundingSummaryProblem

    data class Maximum(val amount: String) : FundingSummaryProblem
}

data class FundingFeesUiState(
    val payTitle: FundingPayTitle,
    /** What the user pays before fees: with the fees it adds up to [pay]. */
    val amount: String,
    val providerFee: String,
    val networkFee: String,
    val totalFee: String,
    val pay: String,
    val rate: FundingRate?,
)

enum class FundingPayTitle {
    YOU_PAY,
    AMOUNT_TO_SEND,
    YOU_SEND,
}

data class FundingRate(
    val symbol: String,
    val perCash: String,
)

data class FundingCountryUi(
    val code: String,
    val flag: String,
    val name: String,
    val currencyName: String?,
    val refused: Boolean,
    /** "$50 CASH" when the country takes no less than that and the amount is below it. */
    val minimum: String?,
    val selected: Boolean,
) {
    /** A country below its minimum can still be picked; the amount sheet then asks for at least that. */
    val enabled: Boolean
        get() = !refused
}

data class FundingCountryUiState(
    val query: String,
    val detected: FundingCountryUi?,
    val supported: ImmutableList<FundingCountryUi>,
    val belowMinimum: ImmutableList<FundingCountryUi>,
    val unsupported: ImmutableList<FundingCountryUi>,
)

data class FundingProvidersUiState(
    val countdown: FundingCountdown?,
    val rows: ImmutableList<FundingProviderRow>,
)

/** How long the prices on screen hold before the core asks every provider again. */
sealed interface FundingCountdown {
    /** No price has arrived yet. */
    data object Pending : FundingCountdown

    /** The prices are being asked for again; the countdown holds at zero. */
    data object Refreshing : FundingCountdown

    /** The core asks again at [at]; the screen counts down to it. */
    data class Until(val at: Instant) : FundingCountdown
}

data class FundingProviderRow(
    val brand: FundingProviderBrand,
    val selected: Boolean,
    val enabled: Boolean,
    val dimmed: Boolean,
    val badge: FundingProviderBadge?,
    val price: FundingProviderPrice?,
)

sealed interface FundingProviderBadge {
    data object LowestPrice : FundingProviderBadge

    data object Unavailable : FundingProviderBadge

    data object CountryUnsupported : FundingProviderBadge

    data class Minimum(val amount: String) : FundingProviderBadge

    data class Maximum(val amount: String) : FundingProviderBadge
}

sealed interface FundingProviderPrice {
    data object Pending : FundingProviderPrice

    /** Asked again: what the last price was for stays, the price loads. */
    data class Updating(val forAmount: String) : FundingProviderPrice

    data class Quoted(val price: String, val forAmount: String) : FundingProviderPrice
}

data class FundingNetworkRow(
    val id: String,
    val name: String,
    val monogram: String,
    val minimum: String?,
)

data class FundingTokenRow(
    val symbol: String,
    val monogram: String,
)

data class FundingTokensUiState(
    val networkName: String?,
    val networkMonogram: String?,
    val rows: ImmutableList<FundingTokenRow>,
)

data class FundingDepositUiState(
    val rail: FundingRail,
    val withdraw: Boolean,
    val started: Boolean,
    val failed: Boolean,
    val isCancelling: Boolean,
    val cancel: FundingCancelUiState?,
    /** The core's `FundingProgress.moneyMoved`: only the provider can end the session; before, a second cancel ends it. */
    val moneyMoved: Boolean,
    /** "$50", as the figure over the milestones. */
    val figure: String,
    /** Once the session started: its milestones, what is happening now, and when the provider last reported. */
    val progress: FundingLiveProgressUi?,
    val content: FundingDepositContent,
) {
    /**
     * The user can ask to cancel, and ask again after a refusal. Before money moved a second cancel ends the session,
     * so a provider that went silent cannot hold it; after, a cancel with the provider waits for its answer.
     */
    val canCancel: Boolean
        get() = started && !failed && when (cancel) {
            null, is FundingCancelUiState.Refused -> true
            FundingCancelUiState.Requested -> !moneyMoved
            else -> false
        }

    /** A second cancel ends the session at once, so it reads differently from the first. */
    val isSecondCancel: Boolean
        get() = cancel == FundingCancelUiState.Requested && !moneyMoved
}

data class FundingLiveProgressUi(
    val timeline: FundingTimelineUi,
    val band: FundingBandUi?,
    val lastHeardAtMs: Long?,
)

/** The line under the milestones; amber when the provider is retrying or late. */
data class FundingBandUi(
    @param:StringRes val text: Int,
    val arg: String?,
    val warning: Boolean,
)

/** Where the user's cancel stands once money has moved, so the provider decides it. */
sealed interface FundingCancelUiState {
    /** With the provider, which has not answered. */
    data object Requested : FundingCancelUiState

    /** The provider will not cancel; the session carries on. */
    data class Refused(val reason: String) : FundingCancelUiState

    /** The provider can cancel, keeping [fee] and returning [refund], both as "$1,000.25 CASH". */

    /** The provider's terms; past [expiresAtMs] the offer has lapsed and can't be accepted. */
    data class Offered(val fee: String, val refund: String, val expiresAtMs: Long? = null) : FundingCancelUiState {
        /** Whole seconds the offer stays open, or `null` when the provider set no deadline. */
        fun secondsLeft(nowMs: Long): Long? = expiresAtMs?.let { ((it - nowMs) / MILLIS_PER_SECOND).coerceAtLeast(0) }

        private companion object {
            const val MILLIS_PER_SECOND = 1_000L
        }
    }

    /** The user accepted and the provider is refunding them. */
    data object Cancelling : FundingCancelUiState
}

sealed interface FundingDepositContent {
    data class Crypto(
        val qrPayload: String,
        val amount: String,
        val exact: Boolean,
        val address: String,
        val shortAddress: String,
        val networkName: String,
    ) : FundingDepositContent

    data class Bank(
        val amount: String,
        val beneficiary: String?,
        val account: String?,
        val bankCode: String?,
        val reference: String,
    ) : FundingDepositContent

    /** Why there is no deposit: no provider, a failed start, or the session failed. */
    data class Problem(val problem: FundingSummaryProblem) : FundingDepositContent

    /** No deposit yet: a provider is being found, or the chosen one is preparing it. */
    data class Waiting(val preparing: Boolean, val card: Boolean) : FundingDepositContent
}
