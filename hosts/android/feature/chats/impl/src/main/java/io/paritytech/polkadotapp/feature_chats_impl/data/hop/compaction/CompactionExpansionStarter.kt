package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class CompactionExpansionStarter @Inject constructor(
    @param:ApplicationContext private val appContext: Context
) {
    fun startExpansion() {
        CompactionExpansionWorker.startExpansionWork(appContext)
    }
}
