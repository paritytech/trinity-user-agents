package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

interface ShareCoinageLogsUseCase {
    suspend operator fun invoke(): Result<Unit>
}
