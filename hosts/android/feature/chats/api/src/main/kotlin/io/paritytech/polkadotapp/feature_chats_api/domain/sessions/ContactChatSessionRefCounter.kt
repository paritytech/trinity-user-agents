package io.paritytech.polkadotapp.feature_chats_api.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.coroutines.flow.Flow

interface ContactChatSessionRefCounter {
    val enabledIds: Flow<Set<AccountId>>
    suspend fun requestSessionsEnabled(accountIds: Set<AccountId>, label: String): ContactChatSessionReference
}

interface ContactChatSessionReference {
    suspend fun release()
}

suspend fun ContactChatSessionRefCounter.requestSessionEnabled(accountId: AccountId, label: String): ContactChatSessionReference =
    requestSessionsEnabled(setOf(accountId), label)

suspend fun <R> ContactChatSessionRefCounter.withSessionsEnabled(accountIds: Set<AccountId>, label: String, block: suspend () -> R): R {
    val ref = requestSessionsEnabled(accountIds, label)
    return try {
        block()
    } finally {
        ref.release()
    }
}

suspend fun <R> ContactChatSessionRefCounter.withSessionEnabled(accountId: AccountId, label: String, block: suspend () -> R): R =
    withSessionsEnabled(setOf(accountId), label, block)
