package io.paritytech.polkadotapp.feature_w3spay_impl.domain

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_w3spay_impl.data.scale.W3sSubmitterPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.AmountPreset
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload
import java.math.BigDecimal
import javax.inject.Inject

/**
 * Builds the [SendEnterAmountPayload] that opens the existing payment screen for a W3S real-time
 * payment, with the amount pre-filled and locked and the statement-store transfer method selected.
 */
class W3sPaymentPayloadFactory @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) {
    suspend fun create(
        amount: BigDecimal,
        topic: ByteArray,
        merchantKey: X25519PublicKey,
        paymentId: String,
        recipientLabel: String,
    ): SendEnterAmountPayload {
        val precision = chainAssetProvider.asset().precision
        val planks = amount.planksFromAmount(precision).value

        val submitterPayload = BinaryScale.encodeToByteArray(
            W3sSubmitterPayload(topic = topic, merchantKey = merchantKey.bytes.value, paymentId = paymentId)
        )

        return SendEnterAmountPayload(
            showTransactionResult = true,
            transferMethod = TransferMethodPayload.CoinsViaSubmitter(
                submitterId = W3S_COINS_SUBMITTER_ID,
                submitterPayload = submitterPayload,
                recipientLabel = recipientLabel,
            ),
            amountPreset = AmountPreset(amount = planks, lockAmount = true),
        )
    }
}
