package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Parks a [ProductSigningContext], opens the signing confirmation screen and awaits the user's
 * decision. Shared by every host-side entry point that needs a confirmation, since neither may
 * depend on the other: `HostApiInteractor` already injects `AccountsProtocol`.
 */
@Singleton
class ProductSigningScreenLauncher @Inject constructor(
    private val signingContextHolder: SigningContextHolder,
    private val productsRouter: ProductsRouter,
) {
    private val reviews = Mutex()

    suspend fun awaitDecision(
        requesterName: String,
        requesterIconUrl: String,
        signingRequestBody: SigningRequestBody,
        signingAccount: SigningAccount,
    ): Result<SignedTransaction> = reviews.withLock {
        val context = ProductSigningContext(
            requesterName = requesterName,
            requesterIconUrl = requesterIconUrl,
            signingRequestBody = signingRequestBody,
            signingAccount = signingAccount,
        )

        withContext(Dispatchers.Main.immediate) {
            signingContextHolder.set(context)
            productsRouter.openSignTransaction()
        }

        context.awaitResult()
    }

}
