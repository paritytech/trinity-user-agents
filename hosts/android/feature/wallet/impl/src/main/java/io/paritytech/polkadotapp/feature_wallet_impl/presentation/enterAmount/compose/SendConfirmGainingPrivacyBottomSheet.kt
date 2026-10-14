@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.feature_coinage_api.presentation.privacy.ConfirmGainingPrivacySpendContent
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain.ConfirmGainingPrivacySpendUserAction

@Composable
fun SendConfirmGainingPrivacyBottomSheet(
    isVisible: Boolean,
    action: ConfirmGainingPrivacySpendUserAction,
    onSendAnyway: () -> Unit,
    onDismiss: () -> Unit,
) {
    NovaModalBottomSheet(
        isVisible = isVisible,
        onDismissRequest = onDismiss,
    ) {
        ConfirmGainingPrivacySpendContent(
            totalTransfer = action.totalTransfer,
            onSendAnyway = onSendAnyway,
            onCancel = onDismiss,
        )
    }
}
