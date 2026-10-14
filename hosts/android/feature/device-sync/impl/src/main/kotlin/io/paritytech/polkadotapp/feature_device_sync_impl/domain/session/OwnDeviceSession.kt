package io.paritytech.polkadotapp.feature_device_sync_impl.domain.session

import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel

/** Inter-own-device transport for one paired device. [dispose] cancels everything. */
class OwnDeviceSession(
    val peer: ActiveSsoSession,
    val communicationSession: CommunicationSession,
    private val scope: CoroutineScope,
) : Identifiable {
    override val identifier: String = peer.id

    fun dispose() {
        scope.cancel()
    }
}
