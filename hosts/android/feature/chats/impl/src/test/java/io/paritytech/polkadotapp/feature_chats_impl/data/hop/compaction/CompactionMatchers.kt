package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import org.mockito.Mockito

fun anyHopTicket(): HopTicket {
    Mockito.argThat<Any?> { true }
    return HopTicket.fromRaw(ByteArray(32))
}
