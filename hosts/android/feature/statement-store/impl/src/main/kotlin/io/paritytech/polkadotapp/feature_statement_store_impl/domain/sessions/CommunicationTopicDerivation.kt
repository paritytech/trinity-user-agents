package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions

import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.createSessionIdParams

private val SESSION_PREFIX = "session".toByteArray(Charsets.UTF_8)

/**
 * Blake2b-256 of [salt] + [params] keyed with [CommunicationEncryption.sharedSecret] — so
 * only the two parties to the session can produce or verify the result. Used to derive
 * session-scoped opaque byte arrays (topics, channel-specific secrets, etc.).
 */
fun generateSharedSessionValue(
    salt: ByteArray,
    params: ByteArray,
    encryption: CommunicationEncryption,
): ByteArray = (salt + params).blake2b256(encryption.sharedSecret.bytes.value)

fun deriveCommunicationTopic(
    sender: SessionAccount,
    receiver: SessionAccount,
    encryption: CommunicationEncryption,
): CommunicationSessionId {
    val params = createSessionIdParams(sender, receiver)
    return generateSharedSessionValue(SESSION_PREFIX, params, encryption)
}
