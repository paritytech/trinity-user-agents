package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import androidx.annotation.StringRes
import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class PaymentSearchSectionUiModel(
    val key: String,
    @StringRes val titleRes: Int,
    val items: ImmutableList<PaymentSearchResultUiModel>,
)

data class PaymentSearchResultUiModel(
    val extractedAddress: ExtractedAddress,
    val avatarModel: AvatarUiModel,
)

@Immutable
sealed interface PaymentSearchResults {
    data class Sections(val sections: ImmutableList<PaymentSearchSectionUiModel>) : PaymentSearchResults

    data object Waiting : PaymentSearchResults

    data object Loading : PaymentSearchResults

    data object Empty : PaymentSearchResults
}

data class SendPaymentUiState(
    val input: String,
    val results: PaymentSearchResults,
)
