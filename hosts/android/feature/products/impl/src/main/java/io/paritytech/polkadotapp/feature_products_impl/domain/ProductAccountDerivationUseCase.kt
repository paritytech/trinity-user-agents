package io.paritytech.polkadotapp.feature_products_impl.domain

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.ProductAccountIdProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.productAccountPath
import io.paritytech.polkadotapp.feature_products_api.model.derivation.productSubtreePath
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class ProductAccountDerivationUseCase @Inject constructor(
    private val accountDerivationUseCase: AccountDerivationUseCase,
) : ProductAccountIdProvider {
    override suspend fun deriveAccountId(productAccountId: ProductAccountId): Result<EncodedPublicKey> {
        return accountDerivationUseCase.deriveAccount(productAccountId.derivationPath())
    }

    override suspend fun deriveProductSubtreePublicKey(productId: ProductId): Result<EncodedPublicKey> {
        return accountDerivationUseCase.deriveAccount(productSubtreePath(productId))
    }

    suspend fun deriveTransactionSignerSource(productAccountId: ProductAccountId): Result<TransactionSignerSource.Signed> {
        return deriveKeypair(productAccountId).map { keypair ->
            val encryption = MultiChainEncryption.Substrate(EncryptionType.SR25519)
            TransactionSignerSource.FromKeyPair(keypair, encryption)
        }
    }

    suspend fun deriveKeypair(productAccountId: ProductAccountId): Result<Sr25519Keypair> {
        return accountDerivationUseCase.deriveKeypair(productAccountId.derivationPath())
            .map { it as Sr25519Keypair }
    }

    private fun ProductAccountId.derivationPath(): String = productAccountPath(ProductId.fromStoredValue(productId), index)
}
