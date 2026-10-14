package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.database.model.ExternalPaymentLocal
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.SelectedVoucherKeysCodec
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import java.math.BigInteger

class ExternalPaymentStageRow(
    val stage: ExternalPaymentLocal.Stage,
    val selectedVoucherKeys: String?,
    val surplusPlanks: BigInteger?,
    val claimedPlanks: BigInteger?,
    val failureReason: String?,
)

fun ExternalPayment.Stage.toRow(codec: SelectedVoucherKeysCodec): ExternalPaymentStageRow = when (this) {
    ExternalPayment.Stage.EnsureVouchers -> emptyRow(ExternalPaymentLocal.Stage.ENSURE_VOUCHERS)

    is ExternalPayment.Stage.AwaitRecycling -> ExternalPaymentStageRow(
        stage = ExternalPaymentLocal.Stage.AWAIT_RECYCLING,
        selectedVoucherKeys = codec.encode(exactVoucherKeys),
        surplusPlanks = null,
        claimedPlanks = null,
        failureReason = null,
    )

    is ExternalPayment.Stage.OffboardVouchers -> ExternalPaymentStageRow(
        stage = ExternalPaymentLocal.Stage.OFFBOARD_VOUCHERS,
        selectedVoucherKeys = codec.encode(selectedVoucherKeys),
        surplusPlanks = surplus.value,
        claimedPlanks = null,
        failureReason = null,
    )

    ExternalPayment.Stage.Completed -> emptyRow(ExternalPaymentLocal.Stage.COMPLETED)

    is ExternalPayment.Stage.PartiallyCompleted -> ExternalPaymentStageRow(
        stage = ExternalPaymentLocal.Stage.PARTIALLY_COMPLETED,
        selectedVoucherKeys = null,
        surplusPlanks = null,
        claimedPlanks = claimed.value,
        failureReason = null,
    )

    is ExternalPayment.Stage.Failed -> ExternalPaymentStageRow(
        stage = ExternalPaymentLocal.Stage.FAILED,
        selectedVoucherKeys = null,
        surplusPlanks = null,
        claimedPlanks = null,
        failureReason = reason,
    )
}

fun ExternalPaymentLocal.toDomainStage(codec: SelectedVoucherKeysCodec): ExternalPayment.Stage = when (stage) {
    ExternalPaymentLocal.Stage.ENSURE_VOUCHERS -> ExternalPayment.Stage.EnsureVouchers

    ExternalPaymentLocal.Stage.AWAIT_RECYCLING -> ExternalPayment.Stage.AwaitRecycling(
        exactVoucherKeys = codec.decode(requireNotNull(selectedVoucherKeys) { "AWAIT_RECYCLING row missing selectedVoucherKeys" }),
    )

    ExternalPaymentLocal.Stage.OFFBOARD_VOUCHERS -> ExternalPayment.Stage.OffboardVouchers(
        selectedVoucherKeys = codec.decode(requireNotNull(selectedVoucherKeys) { "OFFBOARD row missing selectedVoucherKeys" }),
        surplus = requireNotNull(surplusPlanks) { "OFFBOARD row missing surplusPlanks" }.intoBalance(),
    )

    ExternalPaymentLocal.Stage.COMPLETED -> ExternalPayment.Stage.Completed

    ExternalPaymentLocal.Stage.PARTIALLY_COMPLETED -> ExternalPayment.Stage.PartiallyCompleted(
        claimed = requireNotNull(claimedPlanks) { "PARTIALLY_COMPLETED row missing claimedPlanks" }.intoBalance(),
    )

    ExternalPaymentLocal.Stage.FAILED -> ExternalPayment.Stage.Failed(failureReason.orEmpty())
}

private fun emptyRow(stage: ExternalPaymentLocal.Stage) = ExternalPaymentStageRow(
    stage = stage,
    selectedVoucherKeys = null,
    surplusPlanks = null,
    claimedPlanks = null,
    failureReason = null,
)
