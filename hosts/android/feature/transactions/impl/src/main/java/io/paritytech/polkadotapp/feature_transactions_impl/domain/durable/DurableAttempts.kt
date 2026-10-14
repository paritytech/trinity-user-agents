package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.DefaultSignedExtensions
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Era
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.findExplicitOrNull
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.util.extrinsicHash
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxRegistrationError
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxAttempt

/**
 * The window is the extrinsic's own: its `CheckMortality` era is what the runtime will actually enforce, so
 * anchoring to anything else would search a range the extrinsic could not have landed in. The anchor is read
 * off the request rather than re-derived — re-deriving it from a head read at registration time can name a
 * different block once the head has crossed a period boundary.
 */
internal fun EnrichedSendableExtrinsic.toAttempt(): DurableTxAttempt {
    val era = mortalEra() ?: throw DurableTxRegistrationError.NotMortal
    val anchor = mortality.eraBlockNumber ?: throw DurableTxRegistrationError.MissingEraAnchor

    return DurableTxAttempt(
        txHash = extrinsicHex.extrinsicHash(),
        checkpoint = CheckpointBlock(anchor, mortality.blockHash.value.toHexString(withPrefix = true)),
        mortalityBlocks = era.period.toLong(),
    )
}

/** The runtime enforces this era, so it is the only window a transaction may be recovered against. */
private fun SendableExtrinsic.mortalEra(): Era.Mortal? =
    extrinsic.findExplicitOrNull(DefaultSignedExtensions.CHECK_MORTALITY) as? Era.Mortal
