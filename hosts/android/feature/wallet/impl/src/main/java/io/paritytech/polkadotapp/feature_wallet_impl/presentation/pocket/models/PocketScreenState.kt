package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

sealed interface PocketScreenState {
    val contentKey: String

    data class List(
        val collectiblesAvailable: Boolean,
        val removalCandidate: PocketCardUiModel.ProductCard?
    ) : PocketScreenState {
        override val contentKey: String get() = "list"
    }

    data class CardDetails(
        val selectedCard: PocketCardUiModel
    ) : PocketScreenState {
        override val contentKey: String get() = selectedCard.id
    }

    data object Collectibles : PocketScreenState {
        override val contentKey: String get() = "collectibles"
    }
}
