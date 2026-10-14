package io.paritytech.polkadotapp.feature_transactions.api.data.tracked

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

sealed interface TrackedExtrinsicStatus {
    val terminal: Boolean

    data object Pending : TrackedExtrinsicStatus {
        override val terminal: Boolean = false
    }

    data object Accepted : TrackedExtrinsicStatus {
        override val terminal: Boolean = false
    }

    data class InBlock(val blockHash: String) : TrackedExtrinsicStatus {
        override val terminal: Boolean = false
    }

    data class Finalized(val blockHash: String) : TrackedExtrinsicStatus {
        override val terminal: Boolean = true
    }

    data class Failed(val message: String) : TrackedExtrinsicStatus {
        override val terminal: Boolean = true
    }
}

@JvmInline
value class ExtrinsicTag(val value: String) {
    companion object {
        /** Joins [parts] with `-`, the canonical tag separator. */
        fun fromParts(vararg parts: Any): ExtrinsicTag = ExtrinsicTag(parts.joinToString(separator = "-"))
    }

    override fun toString(): String {
        return value
    }
}

class ActiveTrackedExtrinsic(
    val tag: ExtrinsicTag,
    val additional: DataByteArray?,
)
