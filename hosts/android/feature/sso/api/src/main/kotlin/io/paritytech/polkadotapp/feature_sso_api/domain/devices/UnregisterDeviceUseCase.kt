package io.paritytech.polkadotapp.feature_sso_api.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface UnregisterDeviceUseCase {
    suspend operator fun invoke(statementAccountId: AccountId): Result<Unit>
}
