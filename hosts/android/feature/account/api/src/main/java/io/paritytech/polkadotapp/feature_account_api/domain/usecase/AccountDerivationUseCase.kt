package io.paritytech.polkadotapp.feature_account_api.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey

interface AccountDerivationUseCase {
    suspend fun deriveAccount(derivationPath: String): Result<EncodedPublicKey>

    suspend fun deriveRootAccount(): Result<EncodedPublicKey>

    suspend fun deriveKeypair(derivationPath: String): Result<Keypair>
}
