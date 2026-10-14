package io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount

import android.os.Parcelable
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressParcel
import kotlinx.parcelize.Parcelize
import java.math.BigInteger

@Parcelize
data class SendEnterAmountPayload(
    val showTransactionResult: Boolean,
    val transferMethod: TransferMethodPayload,
    val amountPreset: AmountPreset?
) : Parcelable

@Parcelize
class AmountPreset(
    val amount: BigInteger,
    val lockAmount: Boolean
) : Parcelable

/**
 * How the entered amount is delivered. Each variant carries exactly the data its delivery method
 * needs — the recipient account for the in-chat / external flows, or the statement-store topic +
 * merchant encryption key for the W3S real-time payment flow.
 */
sealed interface TransferMethodPayload : Parcelable {
    /**
     * Recipient receives coins via an in-chat memo transfer. User must have an established chat with recipient.
     */
    @Parcelize
    data class CoinsViaChat(val recipient: ExtractedAddressParcel) : TransferMethodPayload

    /**
     * Recipient receives unshielded pusd. Recipient can be any account.
     */
    @Parcelize
    data class UnloadIntoExternal(val recipient: ExtractedAddressParcel) : TransferMethodPayload

    /**
     * Coins are handed to the [io.paritytech.polkadotapp.feature_coinage_api.domain.submitter.CoinsSubmitter]
     * registered under [submitterId]. [submitterPayload] is opaque to the send flow — the submitter
     * (e.g. the W3S feature) defines and decodes it. [recipientLabel] is the human-readable recipient
     * shown on the amount screen (e.g. the merchant name, falling back to the cash-register serial).
     */
    @Parcelize
    data class CoinsViaSubmitter(
        val submitterId: String,
        val submitterPayload: ByteArray,
        val recipientLabel: String,
    ) : TransferMethodPayload
}
