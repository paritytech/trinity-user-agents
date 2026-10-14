package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded

data class ContactWithChatRequestLocal(
    @Embedded val contact: ContactLocal,
    @Embedded(prefix = "request_") val chatRequest: ChatRequestLocal?
)
