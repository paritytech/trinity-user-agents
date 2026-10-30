package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.database.dao.ChatIdentitySigningConsentDao
import io.paritytech.polkadotapp.database.model.ChatIdentitySigningConsentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningConsent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface IdentitySigningConsentRepository {
    suspend fun get(contactAccountId: AccountId): IdentitySigningConsent?

    fun subscribe(contactAccountId: AccountId): Flow<IdentitySigningConsent?>

    suspend fun set(contactAccountId: AccountId, consent: IdentitySigningConsent)

    suspend fun clear(contactAccountId: AccountId)
}

class RealIdentitySigningConsentRepository @Inject constructor(
    private val dao: ChatIdentitySigningConsentDao,
) : IdentitySigningConsentRepository {
    override suspend fun get(contactAccountId: AccountId): IdentitySigningConsent? {
        return dao.get(contactAccountId.value)?.state?.toDomain()
    }

    override fun subscribe(contactAccountId: AccountId): Flow<IdentitySigningConsent?> {
        return dao.subscribe(contactAccountId.value).map { it?.state?.toDomain() }
    }

    override suspend fun set(contactAccountId: AccountId, consent: IdentitySigningConsent) {
        dao.upsert(ChatIdentitySigningConsentLocal(contactAccountId.value, consent.toLocal()))
    }

    override suspend fun clear(contactAccountId: AccountId) {
        dao.delete(contactAccountId.value)
    }

    private fun ChatIdentitySigningConsentLocal.State.toDomain(): IdentitySigningConsent = when (this) {
        ChatIdentitySigningConsentLocal.State.REQUESTED -> IdentitySigningConsent.REQUESTED
        ChatIdentitySigningConsentLocal.State.GRANTED -> IdentitySigningConsent.GRANTED
        ChatIdentitySigningConsentLocal.State.DECLINED -> IdentitySigningConsent.DECLINED
    }

    private fun IdentitySigningConsent.toLocal(): ChatIdentitySigningConsentLocal.State = when (this) {
        IdentitySigningConsent.REQUESTED -> ChatIdentitySigningConsentLocal.State.REQUESTED
        IdentitySigningConsent.GRANTED -> ChatIdentitySigningConsentLocal.State.GRANTED
        IdentitySigningConsent.DECLINED -> ChatIdentitySigningConsentLocal.State.DECLINED
    }
}
