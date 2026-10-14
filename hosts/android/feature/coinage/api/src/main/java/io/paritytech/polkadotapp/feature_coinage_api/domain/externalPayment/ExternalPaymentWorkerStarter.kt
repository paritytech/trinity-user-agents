package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

/**
 * Triggers the external-payment worker. Called on app start (force-retry any payments stuck
 * in non-terminal stages) and whenever a new payment is initiated.
 */
interface ExternalPaymentWorkerStarter {
    fun start()
}
