package io.paritytech.polkadotapp.feature_usernames_api.domain.usecase

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import kotlinx.coroutines.flow.Flow

interface ObserveAccountOnboardingStatusUseCase {
    operator fun invoke(): Flow<AccountOnboardingStatus>
}
