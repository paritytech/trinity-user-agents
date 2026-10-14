package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementTopic
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccountParams
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementTimestamp
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.createSessionIdParams
import kotlinx.serialization.Serializable

object ChatRequestTopicDerivation {
    private const val CONTEXT = "chat-request"

    /**
     * Topic 1: Daily-partitioned topic for bounded sync and polling.
     * hash(("chat-request", acceptor_account_id, day))
     */
    fun deriveDayTopic(acceptorAccountId: AccountId, day: Long): StatementTopic {
        val dayTopic = DayTopic(
            context = CONTEXT,
            acceptorAccountId = acceptorAccountId.value,
            day = day.toULong()
        )
        return BinaryScale.encodeToByteArray(dayTopic).blake2b256()
    }

    /**
     * Topic 2: Full historical topic for complete sync.
     * hash(("chat-request", acceptor_account_id))
     */
    fun deriveFullTopic(acceptorAccountId: AccountId): StatementTopic {
        val fullTopic = FullTopic(
            context = CONTEXT,
            acceptorAccountId = acceptorAccountId.value
        )
        return BinaryScale.encodeToByteArray(fullTopic).blake2b256()
    }

    /**
     * Topic 3: Session-based fallback topic.
     * khash(sharedSecret, "chat-request" + SessionIdParams)
     *
     * Used for direct communication between two accounts that already
     * have an established shared secret.
     */
    fun deriveSessionTopic(
        sharedSecret: ByteArray,
        acceptor: SessionAccountParams,
        requester: SessionAccountParams
    ): StatementTopic {
        val sessionIdParams = createSessionIdParams(requester, acceptor)
        val salt = CONTEXT.toByteArray(Charsets.UTF_8)
        val sessionId = salt + sessionIdParams
        return sessionId.blake2b256(key = sharedSecret)
    }

    /**
     * Calculate day number from Unix timestamp (seconds).
     * Day number is the number of days since the protocol epoch.
     */
    fun calculateDay(unixTimestampSeconds: Long): Long {
        return StatementTimestamp.calculateDay(unixTimestampSeconds)
    }

    /**
     * Get the current day number.
     */
    fun getCurrentDay(): Long {
        return StatementTimestamp.currentDay()
    }
}

@Serializable
private class DayTopic(
    val context: String,
    val acceptorAccountId: ByteArray,
    val day: ULong
)

@Serializable
private class FullTopic(
    val context: String,
    val acceptorAccountId: ByteArray
)
