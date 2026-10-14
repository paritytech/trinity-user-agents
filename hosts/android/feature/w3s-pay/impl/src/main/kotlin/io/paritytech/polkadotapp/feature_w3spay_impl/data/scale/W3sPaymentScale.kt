package io.paritytech.polkadotapp.feature_w3spay_impl.data.scale

import androidx.annotation.Keep
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import kotlinx.serialization.Serializable

/**
 * The W3S-specific data carried inside the opaque `submitterPayload` of a
 * `TransferMethodPayload.CoinsViaSubmitter`. Built when a `pay-w3s` deeplink / DSFinV-K receipt is
 * scanned and decoded back by [io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3sCoinsSubmitter].
 */
@Serializable
@Keep
class W3sSubmitterPayload(
    val topic: ByteArray,
    val merchantKey: ByteArray,
    val paymentId: String,
)

/**
 * Plaintext payment payload published to the merchant via the statement store, before encryption.
 * Field order is part of the wire contract with the merchant decoder — do not reorder.
 */
@Serializable
@Keep
class W3sPaymentDataV1(
    /** Decimal string with a "." separator and exactly two decimal places. */
    val amount: String,
    /** Unix timestamp in milliseconds. */
    val timestamp: ULong,
    /** Secret keys of the coins from the transfer memo. */
    val coins: List<ByteArray>,
    /** The payment id. */
    val id: String,
)

/**
 * ECIES envelope put into the statement `data`: the AES-256-GCM ciphertext (IV ‖ ciphertext ‖ tag)
 * of a SCALE-encoded [W3sPaymentDataV1] together with the ephemeral X25519 public key the merchant
 * needs to reconstruct the shared secret. Mirrors the SSO pairing `HandshakeAnswerV1Scale`.
 */
@Serializable
@Keep
class W3sEncryptedPayloadV1(
    val encryptedData: ByteArray,
    val ephemeralPublicKey: X25519PublicKeyScale,
)
