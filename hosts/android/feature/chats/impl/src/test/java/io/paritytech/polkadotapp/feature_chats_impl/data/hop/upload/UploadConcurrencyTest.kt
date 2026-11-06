package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.app.ActivityManager
import android.content.Context
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.MAX_IN_FLIGHT
import org.junit.Assert.assertEquals
import org.junit.Test

class UploadConcurrencyTest {
    private val context: Context = mockk()
    private val activityManager: ActivityManager = mockk()

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
        every { context.getSystemService(Context.ACTIVITY_SERVICE) } returns activityManager
        every { activityManager.memoryClass } returns megabytes
    }
}
