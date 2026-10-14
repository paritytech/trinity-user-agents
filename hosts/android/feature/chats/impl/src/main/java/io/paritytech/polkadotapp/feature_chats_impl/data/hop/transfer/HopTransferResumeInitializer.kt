package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction.CompactionExpansionStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.download.FileDownloadStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload.FileUploadStarter
import javax.inject.Inject

class HopTransferResumeInitializer @Inject constructor(
    private val downloadStarter: FileDownloadStarter,
    private val uploadStarter: FileUploadStarter,
    private val compactionExpansionStarter: CompactionExpansionStarter
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        downloadStarter.startDownloading()
        uploadStarter.startUploading()
        compactionExpansionStarter.startExpansion()
    }
}
