package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Tells if this host stages the attachments of a room that declares `preimage_attachments` (07-LATER §7). The host
 * stages them when it lists the feature in `ChatHostCapabilities`.
 *
 * The emulator launcher of a debug build can also turn staging on while the feature is not listed, so that it drives
 * S57 to S60 before the rows pass. The products still read the same capabilities.
 */
@Singleton
class ProductAttachmentStagingGate @Inject constructor() {
    @Volatile
    private var servedForE2E = false

    val isServed: Boolean
        get() = servedForE2E || ChatHostCapabilities.PREIMAGE_ATTACHMENTS in ChatHostCapabilities.features

    val sealsServed: Boolean
        get() = servedForE2E || ChatHostCapabilities.SEALED_ATTACHMENTS in ChatHostCapabilities.features

    /** Only the debug E2E hook calls this. */
    fun serveForE2E() {
        servedForE2E = true
    }
}
