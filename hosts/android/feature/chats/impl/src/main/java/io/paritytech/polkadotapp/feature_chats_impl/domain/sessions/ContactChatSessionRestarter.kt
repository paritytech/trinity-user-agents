package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface ContactChatSessionRestarter {
    /**
     * Disposes the running session of [accountId] and starts a fresh one for the same contact, so whatever the old
     * one still had queued in memory is dropped. No-op when [accountId] has no running session.
     */
    suspend fun restartSession(accountId: AccountId)
}
