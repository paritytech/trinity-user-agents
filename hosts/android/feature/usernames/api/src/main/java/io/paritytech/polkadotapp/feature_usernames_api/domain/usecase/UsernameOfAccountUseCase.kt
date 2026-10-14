package io.paritytech.polkadotapp.feature_usernames_api.domain.usecase

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.StoredUsername
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first

interface UsernameOfAccountUseCase {
    operator fun invoke(): Flow<StoredUsername?>

    fun initiallyClaimedLightUsername(): Flow<Username?>

    suspend fun getUsername(): Result<StoredUsername?>
}

suspend fun UsernameOfAccountUseCase.await(): StoredUsername = invoke()
    .filterNotNull()
    .first()
