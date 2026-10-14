package io.paritytech.polkadotapp.feature_account_api.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain

interface SharedSecretDerivationUseCase {
    suspend fun deriveForDomain(domain: SharedSecretDerivationDomain): X25519KeyPair

    suspend fun generateOneTimeUse(): X25519KeyPair
}
