package io.paritytech.polkadotapp.feature_transactions.api.data

import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic

/**
 * A built extrinsic carrying the [Mortality] it was actually signed with.
 *
 * The era anchor is part of the signed payload but not of the transmitted bytes, so a caller that must
 * record it — to know which window the extrinsic can land in — would otherwise have to re-derive it from
 * the chain and could pick a different block than the one the extrinsic committed to.
 */
interface EnrichedSendableExtrinsic : SendableExtrinsic {
    val mortality: Mortality
}

fun SendableExtrinsic.withMortality(mortality: Mortality): EnrichedSendableExtrinsic {
    return RealEnrichedSendableExtrinsic(this, mortality)
}

private class RealEnrichedSendableExtrinsic(
    delegate: SendableExtrinsic,
    override val mortality: Mortality,
) : EnrichedSendableExtrinsic, SendableExtrinsic by delegate
