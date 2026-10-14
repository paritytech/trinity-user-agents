package io.paritytech.polkadotapp.feature_usernames_api.domain.usecase

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.FoundUser

interface SearchUsernamesUseCase {
    suspend operator fun invoke(query: String): Result<List<FoundUser>>
}
