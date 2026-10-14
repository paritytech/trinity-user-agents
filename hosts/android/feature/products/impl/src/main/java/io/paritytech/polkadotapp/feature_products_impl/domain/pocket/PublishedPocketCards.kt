package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import javax.inject.Inject

sealed class PocketPublishError(message: String) : Throwable(message) {
    data object NoPocket : PocketPublishError("product publishes no Pocket cards")

    data object UnknownCard : PocketPublishError("product publishes no such card")

    // Variants are singletons, so a captured trace would point at classloading, not the failure.
    override fun fillInStackTrace(): Throwable = this
}

data class PublishedPocketCard(
    val product: Product,
    val definition: PocketCardDefinition,
)

/** Looks a card up in the worker manifest of the product that claims to back it. */
class PublishedPocketCards @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
) {
    suspend fun find(productId: ProductId, cardId: PocketCardId): Result<PublishedPocketCard> =
        resolveProductUseCase.resolve(productId).flatMap { resolved ->
            val worker = resolved.executables.worker
            if (worker == null || !worker.includesPocket) return@flatMap Result.failure(PocketPublishError.NoPocket)

            worker.pocketCards.firstOrNull { it.id == cardId }
                ?.let { Result.success(PublishedPocketCard(resolved.product, it)) }
                ?: Result.failure(PocketPublishError.UnknownCard)
        }
}
