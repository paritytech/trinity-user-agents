package io.paritytech.polkadotapp.feature_account_api.domain.usecase

interface CreateNewAccountUseCase {
    suspend operator fun invoke(): Result<Unit>
}
