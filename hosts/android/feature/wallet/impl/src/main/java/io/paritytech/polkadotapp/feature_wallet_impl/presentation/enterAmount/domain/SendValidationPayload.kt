package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.TransferMethod
import java.math.BigDecimal

data class SendValidationPayload(
    val value: BigDecimal,
    val transferMethod: TransferMethod,
)
