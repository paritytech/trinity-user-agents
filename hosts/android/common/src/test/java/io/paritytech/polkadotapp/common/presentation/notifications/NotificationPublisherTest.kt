package io.paritytech.polkadotapp.common.presentation.notifications

import androidx.core.app.NotificationManagerCompat
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationPublisherTest {
    /**
     * Publishing reports whether the notification reached the person, and telemetry that counts a
     * notification as shown reads that answer.
     */
    @Test
    fun `a notification posted to a blocked channel does not reach the person`() {
        assertFalse(channelReachesThePerson(NotificationManagerCompat.IMPORTANCE_NONE))
    }

    @Test
    fun `a notification posted to a channel that does not exist does not reach the person`() {
        assertFalse(channelReachesThePerson(null))
    }

    @Test
    fun `a notification posted to a live channel reaches the person`() {
        assertTrue(channelReachesThePerson(NotificationManagerCompat.IMPORTANCE_MIN))
        assertTrue(channelReachesThePerson(NotificationManagerCompat.IMPORTANCE_DEFAULT))
        assertTrue(channelReachesThePerson(NotificationManagerCompat.IMPORTANCE_HIGH))
    }
}
