package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

enum class AllowanceSystem(val slug: String) {
    BULLETIN("bulletin"),
    STATEMENT_STORE("statement-store"),
}

class AllowanceAccountDerivation @Inject constructor(
    private val accountDerivationUseCase: AccountDerivationUseCase,
) {
    suspend fun deriveSlotKey(system: AllowanceSystem, productId: ProductId): Result<SlotAccountKey> {
        return accountDerivationUseCase.deriveKeypair(allowancePath(system, productId))
            .mapCatching { keypair ->
                require(keypair is Sr25519Keypair) { "Allowance accounts must be Sr25519" }
                SlotAccountKey.fromKeypair(keypair)
            }
    }

    private fun allowancePath(system: AllowanceSystem, productId: ProductId): String {
        return "//allowance//${system.slug}//${productId.value}"
    }
}
