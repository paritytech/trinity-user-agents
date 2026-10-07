package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject
import javax.inject.Singleton

/**
 * One person the user may hand to a product.
 *
 * Carries the account so the answer needs no second lookup, and a name only for
 * the row to read; a contact the app has no name for is still pickable.
 */
class ContactPickOption(
    val account: ByteArray,
    val displayName: String?,
)

/** What the contact picker shows: who is asking, and whom they may pick. */
class ContactPickRequest(
    val productId: String,
    val options: List<ContactPickOption>,
)

/** Opens the contact picker for a product and waits for the person the user chose. */
@Singleton
class TrUAPIContactPicks @Inject constructor(
    private val productsRouter: ProductsRouter,
) : TrUAPIPrompts<ContactPickRequest, ContactPickOption?>(unanswered = null) {
    override suspend fun open() = productsRouter.openTrUAPIContactPick()

    override suspend fun close() = productsRouter.closeTrUAPIContactPick()
}
