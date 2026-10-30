package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.app.ActivityManager
import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import javax.inject.Inject

class UploadConcurrency @Inject constructor(
    @param:ApplicationContext private val context: Context
) {
    fun maxInFlight(): Int {
        val activityManager = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        val heap = activityManager.memoryClass.megabytes
        val uploadsThatFit = (heap - RESERVED_HEAP).inWholeMegabytes / HEAP_PER_UPLOAD.inWholeMegabytes

        return uploadsThatFit.toInt().coerceIn(1, MAX_CONCURRENT_UPLOADS)
    }

    private companion object {
        const val MAX_CONCURRENT_UPLOADS = 10

        // Ten 11.4 MB videos uploading on a 256 MB phone peaked at 226,772 kB, about 20 MB each above the idle app
        val HEAP_PER_UPLOAD = 20.megabytes

        val FULL_CONCURRENCY_HEAP = 256.megabytes

        val RESERVED_HEAP = (FULL_CONCURRENCY_HEAP.inWholeMegabytes - MAX_CONCURRENT_UPLOADS * HEAP_PER_UPLOAD.inWholeMegabytes).megabytes
    }
}
