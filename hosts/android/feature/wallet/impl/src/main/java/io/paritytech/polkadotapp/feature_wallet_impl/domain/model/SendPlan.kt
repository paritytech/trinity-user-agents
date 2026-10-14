package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan

sealed interface SendPlan {
    data class Coinage(val plan: TransferPlan) : SendPlan
    data class External(val plan: ExternalPaymentPlan) : SendPlan
}
