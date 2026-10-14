package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class FileDownloadStarter @Inject constructor(
    @param:ApplicationContext private val appContext: Context
) {
    fun startDownloading() {
        FileDownloadWorker.startDownloadingWork(appContext)
    }
}
