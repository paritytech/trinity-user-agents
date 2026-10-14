package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentWorkerStarter
import javax.inject.Inject

class RealExternalPaymentWorkerStarter @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
) : ExternalPaymentWorkerStarter {
    override fun start() {
        ExternalPaymentWorker.start(appContext)
    }
}
