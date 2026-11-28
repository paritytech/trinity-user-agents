package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.presentation.funding.FundingReasonText
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import kotlinx.collections.immutable.ImmutableList
import uniffi.truapi.FundingCancelAvailability
import uniffi.truapi.FundingDetail
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import kotlin.time.Instant

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

data class FundingAmountUiState(
    val isWithdraw: Boolean,
    val rails: ImmutableList<FundingRailTab>,
    val amount: String,
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

sealed interface FundingEta {
    data class Minutes(val count: Int) : FundingEta

    data class Hours(val count: Int) : FundingEta

    data class Days(val count: Int) : FundingEta
}

sealed interface FundingSummaryProblem {
    data object StartFailed : FundingSummaryProblem

    data object NoProvider : FundingSummaryProblem

    data object QuotesFailed : FundingSummaryProblem

    data class QuoteRefused(val text: FundingReasonText) : FundingSummaryProblem

    data class Failed(val reason: FundingFailureReason) : FundingSummaryProblem

    data class Minimum(val amount: String) : FundingSummaryProblem

    data class Maximum(val amount: String) : FundingSummaryProblem
}

data class FundingFeesUiState(
    val payTitle: FundingPayTitle,
    // Before fees; with the fees it adds up to [pay].
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
    val minimum: String?,
    val selected: Boolean,
) {
    // A country below its minimum can still be picked; the amount sheet then asks for at least that.
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
    val now: Instant,
)

sealed interface FundingCountdown {
    data object Pending : FundingCountdown

    data object Refreshing : FundingCountdown

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

    data class Updating(val forAmount: String) : FundingProviderPrice

    data class Quoted(val price: String, val forAmount: String) : FundingProviderPrice
}

data class FundingNetworkRow(
    val id: String,
    val name: String,
    val icon: FundingCryptoIcon,
    val minimum: String?,
)

data class FundingTokenRow(
    val symbol: String,
    val icon: FundingCryptoIcon,
)

data class FundingTokensUiState(
    val networkName: String?,
    val networkIcon: FundingCryptoIcon?,
    val rows: ImmutableList<FundingTokenRow>,
)

data class FundingDepositUiState(
    val rail: FundingRail,
    val withdraw: Boolean,
    val started: Boolean,
    val failed: Boolean,
    val isCancelling: Boolean,
    val cancel: FundingCancelUiState?,
    val moneyMoved: Boolean,
    val cancelAvailability: FundingCancelAvailability,
    val figure: String,
    val progress: FundingLiveProgressUi?,
    val content: FundingDepositContent,
    val now: Instant,
) {
    val canCancel: Boolean
        get() = started && !failed && cancelAvailability != FundingCancelAvailability.UNAVAILABLE

    val isSecondCancel: Boolean
        get() = cancelAvailability == FundingCancelAvailability.AVAILABLE_AGAIN
}

data class FundingLiveProgressUi(
    val timeline: FundingTimelineUi,
    val band: FundingBandUi?,
    val lastHeardAt: Instant?,
    val detail: FundingDetail?,
)

data class FundingBandUi(
    @param:StringRes val text: Int,
    val arg: String?,
    val warning: Boolean,
)

sealed interface FundingCancelUiState {
    data object Requested : FundingCancelUiState

    data class Refused(val reason: String) : FundingCancelUiState

    data class Offered(val fee: String, val refund: String, val expiresAt: Instant?) : FundingCancelUiState {
        fun secondsLeft(now: Instant): Long? = expiresAt?.let { (it - now).inWholeSeconds.coerceAtLeast(0) }
    }

    data object Cancelling : FundingCancelUiState
}

sealed interface FundingDepositContent {
    data class Crypto(
        val qrPayload: String,
        val amount: String,
        val exact: Boolean,
        val address: String,
        val networkName: String,
        val networkIcon: FundingCryptoIcon,
    ) : FundingDepositContent

    data class Bank(
        val amount: String,
        val beneficiary: String?,
        val account: String?,
        val bankCode: String?,
        val reference: String,
    ) : FundingDepositContent

    data class Problem(val problem: FundingSummaryProblem) : FundingDepositContent

    // The provider takes the payment in its own screen, has not reported the address yet, or already has the funds.
    data object ProgressOnly : FundingDepositContent

    data class Waiting(val preparing: Boolean, val card: Boolean) : FundingDepositContent
}
