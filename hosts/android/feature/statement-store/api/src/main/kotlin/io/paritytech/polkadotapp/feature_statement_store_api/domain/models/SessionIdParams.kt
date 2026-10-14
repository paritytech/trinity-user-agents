package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import java.nio.ByteBuffer

private val SEPARATOR = "/".toByteArray(Charsets.UTF_8)

typealias SessionIdParams = ByteArray

class SessionAccountParams(val accountId: AccountId, val pin: String?)

fun createSessionIdParams(
    accountAParams: SessionAccountParams,
    accountBParams: SessionAccountParams
): ByteArray {
    val pinBytesA = accountAParams.pin?.toByteArray(Charsets.UTF_8) ?: byteArrayOf()
    val pinBytesB = accountBParams.pin?.toByteArray(Charsets.UTF_8) ?: byteArrayOf()

    val accountIdBytesA = accountAParams.accountId.value
    val accountIdBytesB = accountBParams.accountId.value

    val totalSize = accountIdBytesA.size +
        accountIdBytesB.size +
        SEPARATOR.size +
        pinBytesA.size +
        SEPARATOR.size +
        pinBytesB.size

    return ByteBuffer.allocate(totalSize)
        .put(accountIdBytesA)
        .put(accountIdBytesB)
        .put(SEPARATOR)
        .put(pinBytesA)
        .put(SEPARATOR)
        .put(pinBytesB)
        .array()
}

/**
 * Creates session ID parameters from two session accounts.
 * The result is a deterministic byte array combining both account IDs and pins.
 */
fun createSessionIdParams(accountA: SessionAccount, accountB: SessionAccount): SessionIdParams {
    return createSessionIdParams(accountA.toSessionAccountParams(), accountB.toSessionAccountParams())
}

private fun SessionAccount.toSessionAccountParams(): SessionAccountParams {
    return SessionAccountParams(accountId, pin)
}
