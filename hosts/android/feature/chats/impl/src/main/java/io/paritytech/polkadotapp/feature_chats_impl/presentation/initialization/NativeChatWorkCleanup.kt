package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import android.content.Context
import androidx.work.WorkManager
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import javax.inject.Inject

/**
 * Cancels the background work that the native person chat scheduled on an install that ran it: the renewal of its chat
 * requests and its HOP transfers. Their workers are gone, so the work could only fail.
 */
internal class NativeChatWorkCleanup @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        val workManager = WorkManager.getInstance(context)
        NATIVE_WORK_NAMES.forEach(workManager::cancelUniqueWork)
    }

    private companion object {
        val NATIVE_WORK_NAMES = listOf("ChatRequestRenewal", "HopCompactionExpansion", "HopFileUpload", "HopFileDownload")
    }
}
