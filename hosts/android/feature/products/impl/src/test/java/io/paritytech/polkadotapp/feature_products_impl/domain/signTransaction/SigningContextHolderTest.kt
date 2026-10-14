package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContext
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

class SigningContextHolderTest {
    @Test
    fun `stale clear from a previous screen keeps the newer context`() {
        val holder = SigningContextHolder()
        val first = signingContext()
        val second = signingContext()

        holder.set(first)
        holder.set(second)
        // The first screen's ViewModel is cleared late (after its dismiss
        // animation), when the holder already belongs to the next request.
        holder.clear(first)

        assertSame(second, holder.get())
    }

    @Test
    fun `owning clear empties the holder`() {
        val holder = SigningContextHolder()
        val context = signingContext()

        holder.set(context)
        holder.clear(context)

        assertNull(holder.get())
    }

    private fun signingContext(): SigningContext = object : SigningContext {
        override val requesterName = "product.dot"
        override val requesterIconUrl = ""
        override val signingRequestBody = SigningRequestBody.SignVrf(
            account = ProductAccountId("product.dot", DerivationIndex32.default()),
            transcriptLabel = ByteArray(0),
            items = emptyList(),
        )
        override val signingAccount = SigningAccount.Product(signingRequestBody.account)

        override suspend fun approve(sign: suspend () -> Result<SignedTransaction>) =
            Result.success(Unit)

        override suspend fun deliverRejection() = Result.success(Unit)
    }
}
