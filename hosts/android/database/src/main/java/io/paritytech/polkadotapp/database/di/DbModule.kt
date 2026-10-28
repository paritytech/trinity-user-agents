package io.paritytech.polkadotapp.database.di

import android.content.Context
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.dao.BrowserTabDao
import io.paritytech.polkadotapp.database.dao.ChatDraftDao
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatRequestDao
import io.paritytech.polkadotapp.database.dao.ChatRequestSyncStateDao
import io.paritytech.polkadotapp.database.dao.ChatRoomDao
import io.paritytech.polkadotapp.database.dao.ChatSearchRecentDao
import io.paritytech.polkadotapp.database.dao.CoinDao
import io.paritytech.polkadotapp.database.dao.ContactDao
import io.paritytech.polkadotapp.database.dao.ContactDeviceDao
import io.paritytech.polkadotapp.database.dao.ExternalPaymentDao
import io.paritytech.polkadotapp.database.dao.FileDownloadDao
import io.paritytech.polkadotapp.database.dao.FileUploadDao
import io.paritytech.polkadotapp.database.dao.ProductDao
import io.paritytech.polkadotapp.database.dao.ProductFundingOperationDao
import io.paritytech.polkadotapp.database.dao.ProductIntegrationDao
import io.paritytech.polkadotapp.database.dao.ProductPermissionGrantDao
import io.paritytech.polkadotapp.database.dao.ProductTopUpDao
import io.paritytech.polkadotapp.database.dao.RecyclerVoucherDao
import io.paritytech.polkadotapp.database.dao.RemovedChatDao
import io.paritytech.polkadotapp.database.dao.RingVrfKeyRegistrationDao
import io.paritytech.polkadotapp.database.dao.ScheduledProductNotificationDao
import io.paritytech.polkadotapp.database.dao.SsoHandledRequestDao
import io.paritytech.polkadotapp.database.dao.SsoSessionDao
import io.paritytech.polkadotapp.database.dao.StatementStoreSlotAllocationDao
import io.paritytech.polkadotapp.database.dao.TokenPriceDao
import io.paritytech.polkadotapp.database.dao.TrackedExtrinsicDao
import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
class DbModule {
    @Provides
    @Singleton
    fun provideAppDatabase(
        @ApplicationContext context: Context,
        preferences: Preferences,
        chatMessageContentMigrations: Set<@JvmSuppressWildcards ChatMessageContentMigration<*, *>>,
    ) = AppDatabase.create(context, preferences, chatMessageContentMigrations)

    @Provides
    @Singleton
    fun provideChainDao(appDatabase: AppDatabase) = appDatabase.chainDao()

    @Provides
    @Singleton
    fun provideStorageDao(appDatabase: AppDatabase) = appDatabase.storageDao()

    @Provides
    @Singleton
    fun provideMetaAccountDao(appDatabase: AppDatabase) = appDatabase.metaAccountDao()

    @Provides
    @Singleton
    fun provideTokenBalanceDao(appDatabase: AppDatabase) = appDatabase.tokenBalanceDao()

    @Provides
    @Singleton
    fun provideChatMessageDao(appDatabase: AppDatabase): ChatMessageDao =
        appDatabase.chatMessageDao()

    @Provides
    @Singleton
    fun provideChatMessageCompactionDao(appDatabase: AppDatabase): ChatMessageCompactionDao =
        appDatabase.chatMessageCompactionDao()

    @Provides
    @Singleton
    fun provideContactDao(appDatabase: AppDatabase): ContactDao = appDatabase.contactDao()

    @Provides
    @Singleton
    fun provideContactDeviceDao(appDatabase: AppDatabase): ContactDeviceDao = appDatabase.contactDeviceDao()

    @Provides
    @Singleton
    fun provideTokenPriceDao(appDatabase: AppDatabase): TokenPriceDao = appDatabase.tokenPriceDao()

    @Provides
    @Singleton
    fun provideSendRecipientDao(appDatabase: AppDatabase) = appDatabase.sendRecipientDao()

    @Provides
    @Singleton
    fun provideChatMessageProcessingDao(appDatabase: AppDatabase) = appDatabase.chatMessageProcessingDao()

    @Provides
    @Singleton
    fun provideProcessedChatMessageDao(appDatabase: AppDatabase) = appDatabase.processedChatMessageDao()

    @Provides
    @Singleton
    fun provideCoinageEntryDao(appDatabase: AppDatabase) = appDatabase.coinageEntryDao()

    @Provides
    @Singleton
    fun provideDurableTxDao(appDatabase: AppDatabase) = appDatabase.durableTxDao()

    @Provides
    @Singleton
    fun provideCoinageInstallationDao(appDatabase: AppDatabase) = appDatabase.coinageInstallationDao()

    @Provides
    @Singleton
    fun provideMessageReactionDao(appDatabase: AppDatabase) = appDatabase.messageReactionsDao()

    @Provides
    @Singleton
    fun provideChatBotStateDao(appDatabase: AppDatabase) = appDatabase.chatBotStateDao()

    @Provides
    @Singleton
    fun provideSsoSessionDao(appDatabase: AppDatabase): SsoSessionDao = appDatabase.ssoSessionDao()

    @Provides
    @Singleton
    fun provideSsoHandledRequestDao(appDatabase: AppDatabase): SsoHandledRequestDao =
        appDatabase.ssoHandledRequestDao()

    @Provides
    @Singleton
    fun provideMessageRevisionDao(appDatabase: AppDatabase) = appDatabase.messageRevisionDao()

    @Provides
    @Singleton
    fun provideMessageNotificationSentDao(appDatabase: AppDatabase) = appDatabase.messageNotificationSentDao()

    @Provides
    @Singleton
    fun provideProductDao(appDatabase: AppDatabase): ProductDao = appDatabase.productDao()

    @Provides
    @Singleton
    fun provideBrowserTabDao(appDatabase: AppDatabase): BrowserTabDao = appDatabase.browserTabDao()

    @Provides
    @Singleton
    fun provideProductFundingOperationDao(appDatabase: AppDatabase): ProductFundingOperationDao =
        appDatabase.productFundingOperationDao()

    @Provides
    @Singleton
    fun provideProductTopUpDao(appDatabase: AppDatabase): ProductTopUpDao =
        appDatabase.productTopUpDao()

    @Provides
    @Singleton
    fun provideChatRequestDao(appDatabase: AppDatabase): ChatRequestDao = appDatabase.chatRequestDao()

    @Provides
    @Singleton
    fun provideChatRequestSyncStateDao(appDatabase: AppDatabase): ChatRequestSyncStateDao = appDatabase.chatRequestSyncStateDao()

    @Provides
    @Singleton
    fun provideCoinDao(appDatabase: AppDatabase): CoinDao = appDatabase.coinDao()

    @Provides
    @Singleton
    fun provideRecyclerVoucherDao(appDatabase: AppDatabase): RecyclerVoucherDao = appDatabase.recyclerVoucherDao()

    @Provides
    @Singleton
    fun provideChatRoomDao(appDatabase: AppDatabase): ChatRoomDao = appDatabase.chatRoomDao()

    @Provides
    @Singleton
    fun provideProductPermissionGrantDao(appDatabase: AppDatabase): ProductPermissionGrantDao = appDatabase.productPermissionGrantDao()

    @Provides
    @Singleton
    fun provideProductIntegrationDao(appDatabase: AppDatabase): ProductIntegrationDao = appDatabase.productIntegrationDao()

    @Provides
    @Singleton
    fun provideFileUploadDao(appDatabase: AppDatabase): FileUploadDao = appDatabase.fileUploadDao()

    @Provides
    @Singleton
    fun provideFileDownloadDao(appDatabase: AppDatabase): FileDownloadDao = appDatabase.fileDownloadDao()

    @Provides
    @Singleton
    fun provideExternalPaymentDao(appDatabase: AppDatabase): ExternalPaymentDao = appDatabase.externalPaymentDao()

    @Provides
    @Singleton
    fun provideStatementStoreSlotAllocationDao(appDatabase: AppDatabase): StatementStoreSlotAllocationDao =
        appDatabase.statementStoreSlotAllocationDao()

    @Provides
    @Singleton
    fun provideScheduledProductNotificationDao(appDatabase: AppDatabase): ScheduledProductNotificationDao =
        appDatabase.scheduledProductNotificationDao()

    @Provides
    @Singleton
    fun provideTrackedExtrinsicDao(appDatabase: AppDatabase): TrackedExtrinsicDao = appDatabase.trackedExtrinsicDao()

    @Provides
    @Singleton
    fun provideRemovedChatDao(appDatabase: AppDatabase): RemovedChatDao = appDatabase.removedChatDao()

    @Provides
    @Singleton
    fun provideChatDraftDao(appDatabase: AppDatabase): ChatDraftDao = appDatabase.chatDraftDao()

    @Provides
    @Singleton
    fun provideChatSearchRecentDao(appDatabase: AppDatabase): ChatSearchRecentDao = appDatabase.chatSearchRecentDao()

    @Provides
    @Singleton
    fun provideRingVrfKeyRegistrationDao(appDatabase: AppDatabase): RingVrfKeyRegistrationDao =
        appDatabase.ringVrfKeyRegistrationDao()
}
