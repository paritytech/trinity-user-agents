package io.paritytech.polkadotapp.feature_sso_api.domain.devices

import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import kotlinx.coroutines.flow.Flow

interface RegisterDeviceUseCase {
    context(diagnostics: StalenessReportCollector)
    operator fun invoke(offer: HandshakeOffer): Flow<RegisterDeviceProgress>
}
