package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import kotlinx.collections.immutable.ImmutableList
import uniffi.truapi.FundingRail
import java.math.BigDecimal

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

    data class Minimum(val amount: String) : FundingSummaryProblem

    data class Maximum(val amount: String) : FundingSummaryProblem
}

data class FundingFeesUiState(
    val payTitle: FundingPayTitle,
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
    val selected: Boolean,
)

data class FundingCountryUiState(
    val query: String,
    val detected: FundingCountryUi?,
    val supported: ImmutableList<FundingCountryUi>,
    val unsupported: ImmutableList<FundingCountryUi>,
)

data class FundingProvidersUiState(
    val countdown: FundingCountdown?,
    val rows: ImmutableList<FundingProviderRow>,
)

sealed interface FundingCountdown {
    data object Pending : FundingCountdown

    data class Remaining(val seconds: Long) : FundingCountdown
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
    val isBank: Boolean,
    val started: Boolean,
    val content: FundingDepositContent,
)

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

    data object NoProvider : FundingDepositContent

    /** No deposit yet: a provider is being found, or the chosen one is preparing it. */
    data class Waiting(val preparing: Boolean) : FundingDepositContent
}
