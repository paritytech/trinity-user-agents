package io.paritytech.polkadotapp.feature_chats_impl.di

import android.content.Context
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.WorkManagerProductAttachmentStageScheduler
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.EngineProductAttachmentRooms
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentRooms
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStageScheduler
import java.io.File
import javax.inject.Singleton

/** The bindings of the outbox of the messages that the user sends to product-source rooms. */
@Module
@InstallIn(SingletonComponent::class)
interface ProductAttachmentModule {
    @Binds
    fun bindStageScheduler(impl: WorkManagerProductAttachmentStageScheduler): ProductAttachmentStageScheduler

    @Binds
    fun bindAttachmentRooms(impl: EngineProductAttachmentRooms): ProductAttachmentRooms

    companion object {
        private const val OUTBOX_FOLDER = "product_attachment_outbox"

        @Provides
        @Singleton
        fun provideAttachmentOutbox(@ApplicationContext context: Context, dispatchers: CoroutineDispatchers): ProductAttachmentOutbox =
            ProductAttachmentOutbox(File(context.filesDir, OUTBOX_FOLDER), dispatchers.io, System::currentTimeMillis)
    }
}
