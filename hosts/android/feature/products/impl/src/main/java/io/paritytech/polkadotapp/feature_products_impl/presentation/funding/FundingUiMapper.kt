package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FUNDING_RAIL_ORDER
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingAmountIssue
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingDirection
import java.math.BigDecimal

private val PRESETS = listOf(BigDecimal("10"), BigDecimal("50"), BigDecimal("100"))

fun FundingFlowState.toAmountUiState(): FundingAmountUiState {
    val withdraw = direction == FundingDirection.OUT
    val available = availableRails

    return FundingAmountUiState(
        isWithdraw = withdraw,
        rails = FUNDING_RAIL_ORDER.map { FundingRailTab(it, selected = it == rail, enabled = it in available) }.toImmutableList(),
        amount = cash.figure(amount),
        digitCount = amountText.count { it.isDigit() },
        hasAmount = amount.signum() > 0,
        available = spendable?.takeIf { withdraw }?.let(cash::label),
        note = amountNote(),
        presets = if (withdraw) persistentListOf() else PRESETS.map { FundingPreset(it, cash.figure(it)) }.toImmutableList(),
        canContinue = canContinue,
    )
}

private fun FundingFlowState.amountNote(): FundingAmountNote? = when (val issue = amountIssue) {
    is FundingAmountIssue.BelowMinimum -> FundingAmountNote.BelowMinimum(cash.label(issue.minimum))
    is FundingAmountIssue.AboveMaximum -> FundingAmountNote.AboveMaximum(cash.label(issue.maximum))
    FundingAmountIssue.NotEnoughBalance -> FundingAmountNote.NotEnough(cash.symbol)
    null -> learnedLimits.minimum?.let {
        FundingAmountNote.Minimum(cash.label(it), withdraw = direction == FundingDirection.OUT)
    }
}
