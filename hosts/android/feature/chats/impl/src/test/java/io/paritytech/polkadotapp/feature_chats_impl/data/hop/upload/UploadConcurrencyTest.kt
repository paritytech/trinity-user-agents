package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.app.ActivityManager
import android.content.Context
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.MAX_IN_FLIGHT
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Assert.assertEquals
import org.junit.Test
import org.mockito.Mockito.mock

class UploadConcurrencyTest {
    private val context: Context = mock(Context::class.java)
    private val activityManager: ActivityManager = mock(ActivityManager::class.java)

    private val uploadConcurrency = UploadConcurrency(context)

    @Test
    fun `ten uploads run at once when the heap is 256 MB`() {
        withHeapClass(megabytes = 256)

        assertEquals(MAX_IN_FLIGHT, uploadConcurrency.maxInFlight())
    }

    @Test
    fun `no more than ten uploads run at once when the heap is 512 MB`() {
        withHeapClass(megabytes = 512)

        assertEquals(MAX_IN_FLIGHT, uploadConcurrency.maxInFlight())
    }

    @Test
    fun `six uploads run at once when the heap is 192 MB`() {
        withHeapClass(megabytes = 192)

        assertEquals(6, uploadConcurrency.maxInFlight())
    }

    @Test
    fun `one upload still runs when the heap is 64 MB`() {
        withHeapClass(megabytes = 64)

        assertEquals(1, uploadConcurrency.maxInFlight())
    }

    private fun withHeapClass(megabytes: Int) {
        whenever(context.getSystemService(Context.ACTIVITY_SERVICE)).thenReturn(activityManager)
        whenever(activityManager.memoryClass).thenReturn(megabytes)
    }
}
