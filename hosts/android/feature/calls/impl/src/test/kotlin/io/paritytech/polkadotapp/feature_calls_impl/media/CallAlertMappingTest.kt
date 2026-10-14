package io.paritytech.polkadotapp.feature_calls_impl.media

import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class CallAlertMappingTest {
    private val routePending = CallAudioDevicesState.EMPTY
    private val routeApplied = CallAudioDevicesState(
        devices = listOf(CallAudioDevice(SPEAKER_ID, CallAudioDeviceType.Speaker, null)),
        selectedId = SPEAKER_ID,
    )

    @Test
    fun `outgoing call waits for the route before ringback`() {
        assertEquals(CallAlert.None, call(CallDirection.Outgoing, CallStatus.Requesting).toCallAlert(routePending))
        assertEquals(CallAlert.None, call(CallDirection.Outgoing, CallStatus.Ringing).toCallAlert(routePending))
    }

    @Test
    fun `outgoing call rings back once the route is applied`() {
        assertEquals(CallAlert.OutgoingRingback, call(CallDirection.Outgoing, CallStatus.Requesting).toCallAlert(routeApplied))
        assertEquals(CallAlert.OutgoingRingback, call(CallDirection.Outgoing, CallStatus.Ringing).toCallAlert(routeApplied))
    }

    @Test
    fun `incoming ringing does not depend on the route`() {
        assertEquals(CallAlert.IncomingRinging, call(CallDirection.Incoming, CallStatus.Ringing).toCallAlert(routePending))
        assertEquals(CallAlert.IncomingRinging, call(CallDirection.Incoming, CallStatus.Ringing).toCallAlert(routeApplied))
    }

    @Test
    fun `no alert outside the ringing phases`() {
        val silentStatuses = listOf(CallStatus.Connecting, CallStatus.Connected(3.seconds), CallStatus.Ended, CallStatus.Failed)
        silentStatuses.forEach { status ->
            assertEquals(CallAlert.None, call(CallDirection.Outgoing, status).toCallAlert(routeApplied))
            assertEquals(CallAlert.None, call(CallDirection.Incoming, status).toCallAlert(routeApplied))
        }
        assertEquals(CallAlert.None, (null as ActiveCallState?).toCallAlert(routeApplied))
    }

    private fun call(direction: CallDirection, status: CallStatus) = ActiveCallState(
        chatId = ChatId.fromRawValue(ByteArray(CHAT_ID_SIZE)),
        offerId = "offer",
        direction = direction,
        status = status,
        initiatedWithVideo = false,
    )

    private companion object {
        const val SPEAKER_ID = 7
        const val CHAT_ID_SIZE = 32
    }
}
