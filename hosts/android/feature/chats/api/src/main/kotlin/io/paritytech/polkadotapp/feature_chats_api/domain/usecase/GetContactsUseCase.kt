package io.paritytech.polkadotapp.feature_chats_api.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.AccountId

/**
 * The user's contacts for the wallet: the native chat's, or the chat product's on an install whose chats it runs
 * (HX2, HA7).
 */
interface GetContactsUseCase {
    suspend operator fun invoke(): List<AddressBookContact>
}

/** A contact as the wallet lists it. Only a native chat with an established chat carries coins today. */
data class AddressBookContact(
    val accountId: AccountId,
    val username: String?,
    val blocked: Boolean,
    val established: Boolean,
    val paysViaChat: Boolean,
)
