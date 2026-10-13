package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

sealed interface PocketCardUiModel {
    val id: String

    data class DigitalDollar(
        val amounts: LoadingState<Amounts>,
        val syncInProgress: Boolean,
        val accountBackupPending: Boolean,
    ) : PocketCardUiModel {
        override val id = "digital_dollar_card"

        val balanceStatus: DigitalDollarBalanceStatus = amounts.dataOrNull.let { loaded ->
            when {
                syncInProgress -> DigitalDollarBalanceStatus.Syncing
                accountBackupPending -> DigitalDollarBalanceStatus.AccountBackupPending
                loaded != null && loaded.notFullyReady -> DigitalDollarBalanceStatus.PartlyReady(loaded.ready)
                else -> DigitalDollarBalanceStatus.TotalOnly
            }
        }

        data class Amounts(
            val balance: TokenAmountModel,
            val ready: TokenAmountModel
        ) {
            val notFullyReady: Boolean
                get() = balance.amount != ready.amount
        }
    }

    /** A card from the Pocket collection, drawn from its product's face tree. Pinned cards cannot be removed. */
    data class ProductCard(
        val key: PocketCardKey,
        val title: String,
        val pinned: Boolean
    ) : PocketCardUiModel {
        override val id = "product_card:${key.productId.value}:${key.cardId.value}"
    }
}
