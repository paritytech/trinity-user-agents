package io.paritytech.polkadotapp.feature_usernames_api.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

interface ResolveUsernamesUseCase {
    suspend operator fun invoke(accountIds: Collection<AccountId>): Result<Map<AccountId, Username?>>
}
