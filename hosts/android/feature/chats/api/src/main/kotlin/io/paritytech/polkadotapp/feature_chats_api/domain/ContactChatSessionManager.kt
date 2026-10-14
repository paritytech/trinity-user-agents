package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface ContactChatSessionManager {
    fun getSession(accountId: AccountId): ContactChatSession?
    fun getAllSessions(): List<ContactChatSession>
    suspend fun awaitSession(accountId: AccountId): ContactChatSession
}
