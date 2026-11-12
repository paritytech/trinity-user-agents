package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Upsert
import io.paritytech.polkadotapp.database.model.ChatIdentitySigningConsentLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface ChatIdentitySigningConsentDao {
    @Upsert
    suspend fun upsert(consent: ChatIdentitySigningConsentLocal)

    @Query("SELECT * FROM chat_identity_signing_consents WHERE contactAccountId = :contactAccountId")
    suspend fun get(contactAccountId: ByteArray): ChatIdentitySigningConsentLocal?

    @Query("SELECT * FROM chat_identity_signing_consents WHERE contactAccountId = :contactAccountId")
    fun subscribe(contactAccountId: ByteArray): Flow<ChatIdentitySigningConsentLocal?>

    @Query("DELETE FROM chat_identity_signing_consents WHERE contactAccountId = :contactAccountId")
    suspend fun delete(contactAccountId: ByteArray)
}
