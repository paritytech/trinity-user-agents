package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.FoundUser
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.SearchUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import javax.inject.Inject

class RealSearchUsernamesUseCase @Inject constructor(
    private val usernameRepository: UsernameRepository,
) : SearchUsernamesUseCase {
    override suspend operator fun invoke(query: String): Result<List<FoundUser>> {
        val normalizedQuery = query.lowercase()

        return usernameRepository.searchUsernames(normalizedQuery)
    }
}
