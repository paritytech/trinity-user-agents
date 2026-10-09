package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.database.model.ProductFundingRecordLocal
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecord
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecordPayout
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import kotlin.time.Instant

internal fun FundingRecord.toLocal() = ProductFundingRecordLocal(
    intent = intent,
    direction = when (direction) {
        FundingDirection.IN -> ProductFundingRecordLocal.Direction.IN
        FundingDirection.OUT -> ProductFundingRecordLocal.Direction.OUT
    },
    rail = rail?.let {
        when (it) {
            FundingRail.CARD -> ProductFundingRecordLocal.Rail.CARD
            FundingRail.BANK -> ProductFundingRecordLocal.Rail.BANK
            FundingRail.CRYPTO -> ProductFundingRecordLocal.Rail.CRYPTO
        }
    },
    asset = asset,
    providerId = providerId,
    requestedPlanks = requestedAmount?.value,
    settledPlanks = settledAmount?.value,
    outcome = when (outcome) {
        FundingRecordOutcome.Delivered -> ProductFundingRecordLocal.Outcome.DELIVERED
        FundingRecordOutcome.Released -> ProductFundingRecordLocal.Outcome.RELEASED
        FundingRecordOutcome.Refunded -> ProductFundingRecordLocal.Outcome.REFUNDED
        is FundingRecordOutcome.Failed -> ProductFundingRecordLocal.Outcome.FAILED
    },
    failureCode = (outcome as? FundingRecordOutcome.Failed)?.code,
    payout = when (payout) {
        FundingRecordPayout.PaidOut -> ProductFundingRecordLocal.Payout.PAID_OUT
        is FundingRecordPayout.Failed -> ProductFundingRecordLocal.Payout.FAILED
        null -> null
    },
    payoutFailureReason = (payout as? FundingRecordPayout.Failed)?.reason,
    transactionId = transactionId,
    reference = reference,
    openedAtMillis = openedAt.toEpochMilliseconds(),
    settledAtMillis = settledAt.toEpochMilliseconds(),
)

internal fun ProductFundingRecordLocal.toRecord() = FundingRecord(
    intent = intent,
    direction = when (direction) {
        ProductFundingRecordLocal.Direction.IN -> FundingDirection.IN
        ProductFundingRecordLocal.Direction.OUT -> FundingDirection.OUT
    },
    rail = rail?.let {
        when (it) {
            ProductFundingRecordLocal.Rail.CARD -> FundingRail.CARD
            ProductFundingRecordLocal.Rail.BANK -> FundingRail.BANK
            ProductFundingRecordLocal.Rail.CRYPTO -> FundingRail.CRYPTO
        }
    },
    asset = asset,
    providerId = providerId,
    requestedAmount = requestedPlanks?.intoBalance(),
    settledAmount = settledPlanks?.intoBalance(),
    outcome = when (outcome) {
        ProductFundingRecordLocal.Outcome.DELIVERED -> FundingRecordOutcome.Delivered
        ProductFundingRecordLocal.Outcome.RELEASED -> FundingRecordOutcome.Released
        ProductFundingRecordLocal.Outcome.REFUNDED -> FundingRecordOutcome.Refunded
        ProductFundingRecordLocal.Outcome.FAILED -> FundingRecordOutcome.Failed(failureCode.orEmpty())
    },
    payout = when (payout) {
        ProductFundingRecordLocal.Payout.PAID_OUT -> FundingRecordPayout.PaidOut
        ProductFundingRecordLocal.Payout.FAILED -> FundingRecordPayout.Failed(payoutFailureReason.orEmpty())
        null -> null
    },
    transactionId = transactionId,
    reference = reference,
    openedAt = Instant.fromEpochMilliseconds(openedAtMillis),
    settledAt = Instant.fromEpochMilliseconds(settledAtMillis),
)
