package io.paritytech.polkadotapp.feature_chats_api.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact

interface GetContactsUseCase {
    suspend operator fun invoke(): List<Contact>
}
