package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * Hands the payment messages of the native chat over to the host's records of the chat product's payments, under the
 * same message id and in the state the native coinage flow left them, so that the chat product's rooms draw them and the
 * host goes on claiming or following the ones that are not over.
 */
interface NativeChatPaymentAdoption {
    /**
     * Keeps a record for every payment message of a contact chat that the host does not hold yet, in the room that
     * [roomOf] gives for the contact. Returns how many it kept; running it again keeps none and changes nothing.
     */
    suspend fun adopt(roomOf: (AccountId) -> ChatId): Int
}
