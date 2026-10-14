package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * One in-flight confirmation and the answer the core is waiting on.
 *
 * The core awaits [TrUAPIConfirmation] decisions on a blocking-pool thread, and
 * suspending there does not stall other TrUAPI traffic, so the prompt may stay
 * open for as long as the user takes.
 */
class TrUAPIConfirmationContext(val confirmation: TrUAPIConfirmation.Prompt) {
    private val decision = CompletableDeferred<Boolean>()

    fun approve() {
        decision.complete(true)
    }

    /** Also the answer for a dismissed sheet, so an abandoned prompt fails closed. */
    fun reject() {
        decision.complete(false)
    }

    suspend fun await(): Boolean = decision.await()
}

/**
 * Parks the in-flight confirmation for the screen to pick up.
 *
 * Mirrors `SigningContextHolder`, including its owner-guarded clear: a prompt's
 * ViewModel is cleared after the dismiss animation, by which time the holder may
 * already carry the next request.
 */
@Singleton
class TrUAPIConfirmationContextHolder @Inject constructor() {
    private var context: TrUAPIConfirmationContext? = null

    fun set(context: TrUAPIConfirmationContext) {
        this.context = context
    }

    fun get(): TrUAPIConfirmationContext? = context

    fun clear(owner: TrUAPIConfirmationContext) {
        if (context === owner) {
            context = null
        }
    }
}

/**
 * Opens the confirmation prompt for a core-initiated action and awaits the
 * user's decision.
 */
// Singleton because the holder is: a launcher per bridge would mean one mutex
// per tab, and up to MAX_LIVE_TABS + Explore bridges racing on the shared holder.
@Singleton
class TrUAPIConfirmationLauncher @Inject constructor(
    private val holder: TrUAPIConfirmationContextHolder,
    private val signingContextHolder: SigningContextHolder,
    private val productsRouter: ProductsRouter,
) {
    // The core fires confirmUserAction concurrently and the holder carries one
    // context, so an overlapping request would overwrite it and strand the
    // first caller. Released even if the sheet is dismissed: onCleared rejects.
    private val oneAtATime = Mutex()

    suspend fun awaitDecision(confirmation: TrUAPIConfirmation): Boolean = oneAtATime.withLock {
        when (confirmation) {
            is TrUAPIConfirmation.Signing -> awaitSigningDecision(confirmation)
            is TrUAPIConfirmation.Prompt -> awaitPromptDecision(confirmation)
        }
    }

    // The app's own signing sheet, so a core-initiated signature is reviewed the
    // same way a native one is, decoded call and all.
    private suspend fun awaitSigningDecision(confirmation: TrUAPIConfirmation.Signing): Boolean {
        val context = TrUAPISigningContext(
            requesterName = confirmation.requesterProductId,
            signingRequestBody = confirmation.request,
            signingAccount = confirmation.request.signingAccount(),
        )
        signingContextHolder.set(context)
        productsRouter.openSignTransaction()
        return context.await()
    }

    private suspend fun awaitPromptDecision(confirmation: TrUAPIConfirmation.Prompt): Boolean {
        val context = TrUAPIConfirmationContext(confirmation)
        holder.set(context)
        productsRouter.openTrUAPIConfirmation()
        return context.await()
    }
}

private fun SigningRequestBody.signingAccount(): SigningAccount = when (this) {
    is SigningRequestBody.ProductAccountSigning -> SigningAccount.Product(account)
    is SigningRequestBody.LegacyAccountSigning -> SigningAccount.Legacy(account)
}
