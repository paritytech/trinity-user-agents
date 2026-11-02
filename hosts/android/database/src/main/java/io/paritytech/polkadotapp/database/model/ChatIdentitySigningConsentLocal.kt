package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.PrimaryKey

/** The user's answer to signing statements of one chat with their identity account when no anonymous slot is free. */
@Entity(
    tableName = "chat_identity_signing_consents",
    foreignKeys = [
        ForeignKey(
            entity = ContactLocal::class,
            parentColumns = ["accountId"],
            childColumns = ["contactAccountId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
)
class ChatIdentitySigningConsentLocal(
    @PrimaryKey val contactAccountId: ByteArray,
    val state: State,
) {
    enum class State {
        REQUESTED,
        GRANTED,
        DECLINED
    }
}
