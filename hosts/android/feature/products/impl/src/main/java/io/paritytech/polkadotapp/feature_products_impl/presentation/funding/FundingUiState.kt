package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

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
