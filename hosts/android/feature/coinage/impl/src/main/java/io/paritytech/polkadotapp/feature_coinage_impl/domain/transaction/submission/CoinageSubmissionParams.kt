package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

const val COINAGE_SPLIT_POLICY_ID = "coinage-split"
const val COINAGE_UNLOAD_POLICY_ID = "coinage-unload"
const val COINAGE_CLAIM_POLICY_ID = "coinage-claim"

/**
 * What a transfer's policy needs beyond the ledger. The transfer gives up once [buildUntil] has passed with its
 * inputs gone from the chain; an attempt proven unable to land is built again only when [retryFailures].
 */
@OptIn(ExperimentalTime::class)
data class TransferSubmissionParams(
    val buildUntil: Instant,
    val retryFailures: Boolean,
)

/** What a claim's policy needs beyond the ledger: the peer's key, which only the payment message carries. */
@OptIn(ExperimentalTime::class)
data class ClaimSubmissionParams(
    val retryUntil: Instant,
    val receivedKey: CoinPrivateKey,
)

/**
 * The persisted shape of the policies' parameters. Stored with every scheduled transaction, so a change to
 * either shape needs a versioned decoder for the rows already written.
 */
@OptIn(ExperimentalTime::class)
object CoinageSubmissionParams {
    val SPLIT_POLICY_ID = SubmissionPolicyId(COINAGE_SPLIT_POLICY_ID)
    val UNLOAD_POLICY_ID = SubmissionPolicyId(COINAGE_UNLOAD_POLICY_ID)
    val CLAIM_POLICY_ID = SubmissionPolicyId(COINAGE_CLAIM_POLICY_ID)

    fun splitPolicy(params: TransferSubmissionParams) = SubmissionPolicy(SPLIT_POLICY_ID, params.encode())

    fun unloadPolicy(params: TransferSubmissionParams) = SubmissionPolicy(UNLOAD_POLICY_ID, params.encode())

    fun claimPolicy(params: ClaimSubmissionParams) = SubmissionPolicy(
        id = CLAIM_POLICY_ID,
        params = BinaryScale.encodeToByteArray(
            ClaimSubmissionParamsScale(params.retryUntil.toEpochMilliseconds(), params.receivedKey)
        ).toDataByteArray(),
    )

    fun decodeTransfer(params: DataByteArray): Result<TransferSubmissionParams> =
        BinaryScale.decodeFromByteArrayCatching<TransferSubmissionParamsScale>(params.value).map { scale ->
            TransferSubmissionParams(Instant.fromEpochMilliseconds(scale.buildUntilMillis), scale.retryFailures)
        }

    fun decodeClaim(params: DataByteArray): Result<ClaimSubmissionParams> =
        BinaryScale.decodeFromByteArrayCatching<ClaimSubmissionParamsScale>(params.value).map { scale ->
            ClaimSubmissionParams(Instant.fromEpochMilliseconds(scale.retryUntilMillis), scale.receivedKey)
        }

    private fun TransferSubmissionParams.encode() = BinaryScale.encodeToByteArray(
        TransferSubmissionParamsScale(buildUntil.toEpochMilliseconds(), retryFailures)
    ).toDataByteArray()
}

/**
 * Whether an attempt that failed with [failure] is worth building again while [now] is before [deadline].
 *
 * An attempt that simply never got included may land if built again, however late. One that was dispatched and
 * failed, or refused outright, would most likely fail the same way — so it is only retried while the window is
 * still open, which is what keeps a failure that always repeats from being rebuilt forever.
 */
@OptIn(ExperimentalTime::class)
internal fun retryableFailure(failure: DurableFailureKind, now: Instant, deadline: Instant): Boolean =
    failure == DurableFailureKind.EXPIRED || now < deadline

@Serializable
private class TransferSubmissionParamsScale(
    val buildUntilMillis: Long,
    val retryFailures: Boolean,
)

@Serializable
private class ClaimSubmissionParamsScale(
    val retryUntilMillis: Long,
    val receivedKey: DataByteArray,
)
