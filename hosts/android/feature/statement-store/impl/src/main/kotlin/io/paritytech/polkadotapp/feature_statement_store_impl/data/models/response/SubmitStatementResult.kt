package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response

import com.google.gson.Gson
import com.google.gson.annotations.SerializedName
import kotlin.jvm.java

sealed class SubmitStatementResult {
    data object New : SubmitStatementResult()
    data object Known : SubmitStatementResult()
    data object KnownExpired : SubmitStatementResult()

    class Rejected(val reason: RejectionReason) : SubmitStatementResult() {
        override fun toString() = "Rejected($reason)"
    }

    class Invalid(val reason: InvalidReason) : SubmitStatementResult() {
        override fun toString() = "Invalid($reason)"
    }

    class InternalError(val error: StatementStoreError) : SubmitStatementResult() {
        override fun toString() = "InternalError($error)"
    }

    val isSuccess: Boolean get() = this is New || this is Known || this is KnownExpired

    companion object {
        fun parse(gson: Gson, result: Any?): SubmitStatementResult {
            val tree = gson.toJsonTree(result)
            val response = gson.fromJson(tree, SubmitResultResponse::class.java)
            return response.toSubmitResultResult()
        }
    }
}

sealed class RejectionReason {
    class DataTooLarge(val submittedSize: Long, val availableSize: Long) : RejectionReason() {
        override fun toString() = "DataTooLarge(submittedSize=$submittedSize, availableSize=$availableSize)"
    }

    class ChannelPriorityTooLow(val submittedExpiry: Long, val minExpiry: Long) : RejectionReason() {
        override fun toString() = "ChannelPriorityTooLow(submittedExpiry=$submittedExpiry, minExpiry=$minExpiry)"
    }

    class AccountFull(val submittedExpiry: Long, val minExpiry: Long) : RejectionReason() {
        override fun toString() = "AccountFull(submittedExpiry=$submittedExpiry, minExpiry=$minExpiry)"
    }

    data object StoreFull : RejectionReason()
    data object NoAllowance : RejectionReason()
}

sealed class InvalidReason {
    data object NoProof : InvalidReason()
    data object BadProof : InvalidReason()

    class EncodingTooLarge(val submittedSize: Long, val maxSize: Long) : InvalidReason() {
        override fun toString() = "EncodingTooLarge(submittedSize=$submittedSize, maxSize=$maxSize)"
    }

    data object AlreadyExpired : InvalidReason()
}

sealed class StatementStoreError {
    class Db(val message: String) : StatementStoreError() {
        override fun toString() = "Db($message)"
    }

    class Decode(val message: String) : StatementStoreError() {
        override fun toString() = "Decode($message)"
    }

    class Storage(val message: String) : StatementStoreError() {
        override fun toString() = "Storage($message)"
    }
}

enum class SubmitStatementOutcome {
    ACCEPTED, RETRIABLE_FAILURE, FATAL_FAILURE
}

fun SubmitStatementResult.determineOutcome(): SubmitStatementOutcome {
    return when (this) {
        is SubmitStatementResult.InternalError -> SubmitStatementOutcome.FATAL_FAILURE

        is SubmitStatementResult.Invalid -> reason.determineOutcome()

        SubmitStatementResult.Known,
        SubmitStatementResult.KnownExpired,
        SubmitStatementResult.New -> SubmitStatementOutcome.ACCEPTED

        is SubmitStatementResult.Rejected -> reason.determineOutcome()
    }
}

@Suppress("UnusedReceiverParameter")
private fun InvalidReason.determineOutcome(): SubmitStatementOutcome {
    return SubmitStatementOutcome.FATAL_FAILURE
}

private fun RejectionReason.determineOutcome(): SubmitStatementOutcome {
    return when (this) {
        is RejectionReason.AccountFull,
        is RejectionReason.ChannelPriorityTooLow,
        is RejectionReason.DataTooLarge -> SubmitStatementOutcome.FATAL_FAILURE

        RejectionReason.NoAllowance,
        RejectionReason.StoreFull -> SubmitStatementOutcome.RETRIABLE_FAILURE
    }
}

private class SubmitResultResponse(
    val status: Status,
    val reason: Reason? = null,
    val submittedSize: Long? = null,
    val availableSize: Long? = null,
    @SerializedName("submitted_expiry")
    val submittedExpiry: Long? = null,
    @SerializedName("min_expiry")
    val minExpiry: Long? = null,
    val maxSize: Long? = null,
    @SerializedName("Db") val db: String? = null,
    @SerializedName("Decode") val decode: String? = null,
    @SerializedName("Storage") val storage: String? = null,
) {
    enum class Status {
        @SerializedName("new")
        NEW,

        @SerializedName("known")
        KNOWN,

        @SerializedName("knownExpired")
        KNOWN_EXPIRED,

        @SerializedName("rejected")
        REJECTED,

        @SerializedName("invalid")
        INVALID,

        @SerializedName("internalError")
        INTERNAL_ERROR,
    }

    enum class Reason {
        @SerializedName("dataTooLarge")
        DATA_TOO_LARGE,

        @SerializedName("channelPriorityTooLow")
        CHANNEL_PRIORITY_TOO_LOW,

        @SerializedName("accountFull")
        ACCOUNT_FULL,

        @SerializedName("storeFull")
        STORE_FULL,

        @SerializedName("noAllowance")
        NO_ALLOWANCE,

        @SerializedName("noProof")
        NO_PROOF,

        @SerializedName("badProof")
        BAD_PROOF,

        @SerializedName("encodingTooLarge")
        ENCODING_TOO_LARGE,

        @SerializedName("alreadyExpired")
        ALREADY_EXPIRED,
    }

    fun toSubmitResultResult(): SubmitStatementResult = when (status) {
        Status.NEW -> SubmitStatementResult.New
        Status.KNOWN -> SubmitStatementResult.Known
        Status.KNOWN_EXPIRED -> SubmitStatementResult.KnownExpired
        Status.REJECTED -> SubmitStatementResult.Rejected(toRejectionReason())
        Status.INVALID -> SubmitStatementResult.Invalid(toInvalidReason())
        Status.INTERNAL_ERROR -> SubmitStatementResult.InternalError(toStoreError())
    }

    private fun toRejectionReason(): RejectionReason = when (reason) {
        Reason.DATA_TOO_LARGE -> RejectionReason.DataTooLarge(submittedSize!!, availableSize!!)
        Reason.CHANNEL_PRIORITY_TOO_LOW -> RejectionReason.ChannelPriorityTooLow(submittedExpiry!!, minExpiry!!)
        Reason.ACCOUNT_FULL -> RejectionReason.AccountFull(submittedExpiry!!, minExpiry!!)
        Reason.STORE_FULL -> RejectionReason.StoreFull
        Reason.NO_ALLOWANCE -> RejectionReason.NoAllowance
        else -> error("Unexpected rejection reason: $reason")
    }

    private fun toInvalidReason(): InvalidReason = when (reason) {
        Reason.NO_PROOF -> InvalidReason.NoProof
        Reason.BAD_PROOF -> InvalidReason.BadProof
        Reason.ENCODING_TOO_LARGE -> InvalidReason.EncodingTooLarge(submittedSize!!, maxSize!!)
        Reason.ALREADY_EXPIRED -> InvalidReason.AlreadyExpired
        else -> error("Unexpected invalid reason: $reason")
    }

    private fun toStoreError(): StatementStoreError = when {
        db != null -> StatementStoreError.Db(db)
        decode != null -> StatementStoreError.Decode(decode)
        storage != null -> StatementStoreError.Storage(storage)
        else -> error("Unknown internal error")
    }
}
