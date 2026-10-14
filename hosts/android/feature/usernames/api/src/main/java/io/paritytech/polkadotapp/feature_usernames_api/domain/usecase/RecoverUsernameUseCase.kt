package io.paritytech.polkadotapp.feature_usernames_api.domain.usecase

interface RecoverUsernameUseCase {
    suspend operator fun invoke(): Result<Boolean>
}
