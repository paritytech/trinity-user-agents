package io.paritytech.polkadotapp.database

import android.content.Context
import androidx.room.AutoMigration
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.database.converters.ChainConverters
import io.paritytech.polkadotapp.database.converters.ExternalApiConverters
import io.paritytech.polkadotapp.database.converters.IntListConverter
import io.paritytech.polkadotapp.database.converters.LongMathConverters
import io.paritytech.polkadotapp.database.dao.BrowserTabDao
import io.paritytech.polkadotapp.database.dao.ChainDao
import io.paritytech.polkadotapp.database.dao.ChatBotStateDao
import io.paritytech.polkadotapp.database.dao.ChatDraftDao
import io.paritytech.polkadotapp.database.dao.ChatMessageCompactionDao
import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.database.dao.ChatMessageProcessingDao
import io.paritytech.polkadotapp.database.dao.ChatMessageReactionDao
import io.paritytech.polkadotapp.database.dao.ChatRequestDao
import io.paritytech.polkadotapp.database.dao.ChatRequestSyncStateDao
import io.paritytech.polkadotapp.database.dao.ChatRoomDao
import io.paritytech.polkadotapp.database.dao.ChatSearchRecentDao
import io.paritytech.polkadotapp.database.dao.CoinDao
import io.paritytech.polkadotapp.database.dao.CoinageEntryDao
import io.paritytech.polkadotapp.database.dao.CoinageInstallationDao
import io.paritytech.polkadotapp.database.dao.ContactDao
import io.paritytech.polkadotapp.database.dao.ContactDeviceDao
import io.paritytech.polkadotapp.database.dao.DurableTxDao
import io.paritytech.polkadotapp.database.dao.ExternalPaymentDao
import io.paritytech.polkadotapp.database.dao.FileDownloadDao
import io.paritytech.polkadotapp.database.dao.FileUploadDao
import io.paritytech.polkadotapp.database.dao.GamePlayersDao
import io.paritytech.polkadotapp.database.dao.MessageNotificationSentDao
import io.paritytech.polkadotapp.database.dao.MessageRevisionDao
import io.paritytech.polkadotapp.database.dao.MetaAccountDao
import io.paritytech.polkadotapp.database.dao.PocketCardDao
import io.paritytech.polkadotapp.database.dao.ProcessedChatMessageDao
import io.paritytech.polkadotapp.database.dao.ProductDao
import io.paritytech.polkadotapp.database.dao.ProductFundingOperationDao
import io.paritytech.polkadotapp.database.dao.ProductIntegrationDao
import io.paritytech.polkadotapp.database.dao.ProductPermissionGrantDao
import io.paritytech.polkadotapp.database.dao.ProductTopUpDao
import io.paritytech.polkadotapp.database.dao.RecyclerVoucherDao
import io.paritytech.polkadotapp.database.dao.RemovedChatDao
import io.paritytech.polkadotapp.database.dao.RingVrfKeyRegistrationDao
import io.paritytech.polkadotapp.database.dao.ScheduledProductNotificationDao
import io.paritytech.polkadotapp.database.dao.SendRecipientDao
import io.paritytech.polkadotapp.database.dao.SsoHandledRequestDao
import io.paritytech.polkadotapp.database.dao.SsoSessionDao
import io.paritytech.polkadotapp.database.dao.StatementStoreSlotAllocationDao
import io.paritytech.polkadotapp.database.dao.StorageDao
import io.paritytech.polkadotapp.database.dao.TokenBalanceDao
import io.paritytech.polkadotapp.database.dao.TokenPriceDao
import io.paritytech.polkadotapp.database.dao.TrackedExtrinsicDao
import io.paritytech.polkadotapp.database.dao.VideoGameBannedPlayerDao
import io.paritytech.polkadotapp.database.dao.VideoGameConnectionAttemptDao
import io.paritytech.polkadotapp.database.dao.VideoGameVoteDao
import io.paritytech.polkadotapp.database.dao.VouchersDao
import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.database.migrations.Migration10To11
import io.paritytech.polkadotapp.database.migrations.Migration12To13
import io.paritytech.polkadotapp.database.migrations.Migration13To14
import io.paritytech.polkadotapp.database.migrations.Migration14To15
import io.paritytech.polkadotapp.database.migrations.Migration15To16
import io.paritytech.polkadotapp.database.migrations.Migration16To17
import io.paritytech.polkadotapp.database.migrations.Migration17To18
import io.paritytech.polkadotapp.database.migrations.Migration18To19
import io.paritytech.polkadotapp.database.migrations.Migration19To20
import io.paritytech.polkadotapp.database.migrations.Migration20To21Spec
import io.paritytech.polkadotapp.database.migrations.Migration21To22
import io.paritytech.polkadotapp.database.migrations.Migration23To24
import io.paritytech.polkadotapp.database.migrations.Migration24To25
import io.paritytech.polkadotapp.database.migrations.Migration26To27
import io.paritytech.polkadotapp.database.migrations.Migration28To29
import io.paritytech.polkadotapp.database.migrations.Migration29To30
import io.paritytech.polkadotapp.database.migrations.Migration2to3
import io.paritytech.polkadotapp.database.migrations.Migration30To31Spec
import io.paritytech.polkadotapp.database.migrations.Migration32To33Spec
import io.paritytech.polkadotapp.database.migrations.Migration34To35Spec
import io.paritytech.polkadotapp.database.migrations.Migration35To36
import io.paritytech.polkadotapp.database.migrations.Migration38To39
import io.paritytech.polkadotapp.database.migrations.Migration3To4
import io.paritytech.polkadotapp.database.migrations.Migration42To43Spec
import io.paritytech.polkadotapp.database.migrations.Migration48To49
import io.paritytech.polkadotapp.database.migrations.Migration54To55Spec
import io.paritytech.polkadotapp.database.migrations.Migration55To56
import io.paritytech.polkadotapp.database.migrations.Migration57To58
import io.paritytech.polkadotapp.database.migrations.Migration60To61
import io.paritytech.polkadotapp.database.migrations.Migration62To63
import io.paritytech.polkadotapp.database.migrations.Migration63To64
import io.paritytech.polkadotapp.database.migrations.Migration65To66
import io.paritytech.polkadotapp.database.model.BrowserTabLocal
import io.paritytech.polkadotapp.database.model.ChatBotStateLocal
import io.paritytech.polkadotapp.database.model.ChatDraftLocal
import io.paritytech.polkadotapp.database.model.ChatMessageCompactionLinkLocal
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessagePendingExpansionLocal
import io.paritytech.polkadotapp.database.model.ChatMessageProcessingLocal
import io.paritytech.polkadotapp.database.model.ChatMessageReactionLocal
import io.paritytech.polkadotapp.database.model.ChatRequestLocal
import io.paritytech.polkadotapp.database.model.ChatRequestSyncStateLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.database.model.ChatSearchRecentLocal
import io.paritytech.polkadotapp.database.model.CoinLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryInputLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryOutputLocal
import io.paritytech.polkadotapp.database.model.CoinageHandoffLocal
import io.paritytech.polkadotapp.database.model.CoinageInstallationLocal
import io.paritytech.polkadotapp.database.model.ContactDeviceLocal
import io.paritytech.polkadotapp.database.model.ContactLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import io.paritytech.polkadotapp.database.model.ExternalPaymentLocal
import io.paritytech.polkadotapp.database.model.FileDownloadLocal
import io.paritytech.polkadotapp.database.model.FileUploadLocal
import io.paritytech.polkadotapp.database.model.GamePlayersLocal
import io.paritytech.polkadotapp.database.model.MessageNotificationSentLocal
import io.paritytech.polkadotapp.database.model.MessageRevisionLocal
import io.paritytech.polkadotapp.database.model.MetaAccountLocal
import io.paritytech.polkadotapp.database.model.PocketCardFaceLocal
import io.paritytech.polkadotapp.database.model.PocketCardLocal
import io.paritytech.polkadotapp.database.model.ProcessedChatMessageLocal
import io.paritytech.polkadotapp.database.model.ProductFundingOperationLocal
import io.paritytech.polkadotapp.database.model.ProductIntegrationLocal
import io.paritytech.polkadotapp.database.model.ProductLocal
import io.paritytech.polkadotapp.database.model.ProductPermissionGrantLocal
import io.paritytech.polkadotapp.database.model.ProductTopUpLocal
import io.paritytech.polkadotapp.database.model.RecyclerVoucherLocal
import io.paritytech.polkadotapp.database.model.RemovedChatLocal
import io.paritytech.polkadotapp.database.model.RingVrfKeyRegistrationLocal
import io.paritytech.polkadotapp.database.model.ScheduledProductNotificationLocal
import io.paritytech.polkadotapp.database.model.SendRecipientLocal
import io.paritytech.polkadotapp.database.model.SsoHandledRequestLocal
import io.paritytech.polkadotapp.database.model.SsoSessionLocal
import io.paritytech.polkadotapp.database.model.SsoSessionMetadataLocal
import io.paritytech.polkadotapp.database.model.StatementStoreSlotAllocationLocal
import io.paritytech.polkadotapp.database.model.StorageEntryLocal
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import io.paritytech.polkadotapp.database.model.TokenPriceLocal
import io.paritytech.polkadotapp.database.model.TrackedExtrinsicLocal
import io.paritytech.polkadotapp.database.model.VideoGameBannedPlayerLocal
import io.paritytech.polkadotapp.database.model.VideoGameConnectionAttemptLocal
import io.paritytech.polkadotapp.database.model.VideoGameVoteLocal
import io.paritytech.polkadotapp.database.model.VoucherLocal
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExplorerLocal
import io.paritytech.polkadotapp.database.model.chain.ChainExternalApiLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal
import io.paritytech.polkadotapp.database.model.chain.ChainNodeLocal
import io.paritytech.polkadotapp.database.model.chain.ChainRuntimeInfoLocal

@Database(
    version = 68,
    entities = [
        ProductFundingOperationLocal::class,
        ChainLocal::class,
        ChainNodeLocal::class,
        ChainAssetLocal::class,
        ChainRuntimeInfoLocal::class,
        ChainExplorerLocal::class,
        ChainExternalApiLocal::class,
        StorageEntryLocal::class,
        MetaAccountLocal::class,
        TokenBalanceLocal::class,
        ContactLocal::class,
        ContactDeviceLocal::class,
        TokenPriceLocal::class,
        SendRecipientLocal::class,
        ChatMessageLocal::class,
        ChatMessageCompactionLinkLocal::class,
        ChatMessagePendingExpansionLocal::class,
        ChatMessageProcessingLocal::class,
        ChatMessageReactionLocal::class,
        ChatBotStateLocal::class,
        VideoGameVoteLocal::class,
        VoucherLocal::class,
        SsoSessionLocal::class,
        SsoSessionMetadataLocal::class,
        MessageRevisionLocal::class,
        MessageNotificationSentLocal::class,
        GamePlayersLocal::class,
        ProductLocal::class,
        ChatRequestLocal::class,
        ChatRequestSyncStateLocal::class,
        CoinLocal::class,
        RecyclerVoucherLocal::class,
        ChatRoomLocal::class,
        ProductPermissionGrantLocal::class,
        VideoGameBannedPlayerLocal::class,
        ProductIntegrationLocal::class,
        FileUploadLocal::class,
        VideoGameConnectionAttemptLocal::class,
        FileDownloadLocal::class,
        ExternalPaymentLocal::class,
        StatementStoreSlotAllocationLocal::class,
        ScheduledProductNotificationLocal::class,
        TrackedExtrinsicLocal::class,
        RemovedChatLocal::class,
        SsoHandledRequestLocal::class,
        ProcessedChatMessageLocal::class,
        ChatDraftLocal::class,
        BrowserTabLocal::class,
        ChatSearchRecentLocal::class,
        RingVrfKeyRegistrationLocal::class,
        DurableTxLocal::class,
        CoinageInstallationLocal::class,
        CoinageEntryInputLocal::class,
        CoinageEntryOutputLocal::class,
        CoinageHandoffLocal::class,
        ProductTopUpLocal::class,
        PocketCardLocal::class,
        PocketCardFaceLocal::class,
    ],
    autoMigrations = [
        // Add ChatMessageReactionLocal
        AutoMigration(from = 1, to = 2),
        // Add ChatBotStateLocal
        AutoMigration(from = 4, to = 5),
        // Add SsoSessionLocal
        AutoMigration(from = 5, to = 6),
        // Add replyToMessageId to ChatMessageLocal
        AutoMigration(from = 6, to = 7),
        // Add isPeerLeft
        AutoMigration(from = 7, to = 8),
        // Add MessageRevisionLocal table
        AutoMigration(from = 8, to = 9),
        // Add MessageNotificationSentLocal table
        AutoMigration(from = 9, to = 10),
        // Add ProductLocal table
        AutoMigration(from = 11, to = 12),
        // Add CoinLocal/VoucherLocal/CoinageTransferDetectionLocal table (replaces chat_payment_detection)
        AutoMigration(from = 20, to = 21, spec = Migration20To21Spec::class),
        // Add ProductPermissionGrantLocal table
        AutoMigration(from = 22, to = 23),
        // Add VideoGameBannedPlayerLocal table
        AutoMigration(from = 27, to = 28),
        // Add FileUploadLocal table + PROCESSING status to ChatMessageLocal
        AutoMigration(from = 30, to = 31, spec = Migration30To31Spec::class),
        // Add VideoGameConnectionAttemptLocal table
        AutoMigration(from = 32, to = 33),
        // Remove text column from FileUploadLocal, add FileDownloadLocal
        AutoMigration(from = 33, to = 34, spec = Migration32To33Spec::class),
        // Rename originalFileSize to fileSize in FileUploadLocal
        AutoMigration(from = 34, to = 35, spec = Migration34To35Spec::class),
        // Add external_payments table
        AutoMigration(from = 36, to = 37),
        // Add ScheduledProductNotificationLocal table
        AutoMigration(from = 39, to = 40),
        // Add tracked_extrinsics table
        AutoMigration(from = 40, to = 41),
        // Add CoinageTransferWalLocal table
        AutoMigration(from = 41, to = 42),
        // Drop name/icon/hostVersion/platformType/platformVersion from sso_sessions, add
        // SsoSessionMetadataLocal, add pendingDevicesFanOut column to contacts
        // Add status + lastUpdate + outgoingUpdateTime columns to sso_sessions and the
        // removed_chats tombstone table (for inter-own-device sync)
        // Add nullable establishedAt column to contacts (timestamp when chat-request was accepted)
        // Add updatedAt column to chat_messages (device-sync watermark for in-place updates)
        // SCALE content migration is also included here
        AutoMigration(from = 42, to = 43, spec = Migration42To43Spec::class),
        // Add SsoHandledRequestLocal table
        AutoMigration(from = 43, to = 44),
        // Add ProcessedChatMessageLocal table
        AutoMigration(from = 45, to = 46),
        // Add offerId column to sso_sessions (device-sync signaling)
        AutoMigration(from = 46, to = 47),
        // Add ChatDraftLocal table
        AutoMigration(from = 47, to = 48),
        // Add localMigratorVersion column to chain_runtimes (forces the metadata v16 re-fetch)
        AutoMigration(from = 49, to = 50),
        // Add ChatSearchRecentLocal table
        AutoMigration(from = 50, to = 51),
        // Add compaction links + pending expansion tables (message compaction)
        AutoMigration(from = 51, to = 52),
        // Add BrowserTabLocal table + ProductLocal.iconUrl column
        AutoMigration(from = 52, to = 53),
        // Add RingVrfKeyRegistrationLocal table (RFC-0024 ring VRF key registry)
        AutoMigration(from = 53, to = 54),
        // Add icon columns + userWorkerUrl to products; drop scriptUrl, contentHash and iconUrl
        AutoMigration(from = 54, to = 55, spec = Migration54To55Spec::class),
        // Add syncedTransactionVersion column to chain_runtimes
        AutoMigration(from = 56, to = 57),
        // Add product_funding_operations table (open funding operations, resumed on app start)
        AutoMigration(from = 58, to = 59),
        // Add product_top_ups table (kept indefinitely and resumed on app start)
        AutoMigration(from = 59, to = 60),
        // Add recycler_vouchers.enteredAt to preserve readiness timers across restarts
        AutoMigration(from = 61, to = 62),
        // Key external_payments by (origin, id); add claimedPlanks for partially claimed payments
        AutoMigration(from = 64, to = 65),
        // Add pocket_cards (cards the user added to the Pocket) and pocket_card_faces (the newest face drawn for a card)
        AutoMigration(from = 66, to = 67),
        // Add chat_messages.sortOrder (local storage order) and its indices
        AutoMigration(from = 67, to = 68),
    ]
)
@TypeConverters(
    LongMathConverters::class,
    ExternalApiConverters::class,
    ChainConverters::class,
    IntListConverter::class,
)
abstract class AppDatabase : RoomDatabase() {
    companion object {
        fun create(
            context: Context,
            preferences: Preferences,
            chatMessageContentMigrations: Set<ChatMessageContentMigration<*, *>> = emptySet(),
        ): AppDatabase {
            return Room
                .databaseBuilder(context.applicationContext, AppDatabase::class.java, "app_v2.db")
                .addAppMigrations(preferences, chatMessageContentMigrations)
                .build()
        }

        // Exposed for re-usage in tests
        fun <T : RoomDatabase> Builder<T>.addAppMigrations(
            preferences: Preferences,
            chatMessageContentMigrations: Set<ChatMessageContentMigration<*, *>>,
        ): Builder<T> {
            return addMigrations(
                Migration2to3(),
                Migration3To4(preferences),
                Migration10To11(),
                Migration12To13(),
                Migration13To14(),
                Migration14To15(),
                Migration15To16(),
                Migration16To17(),
                Migration17To18(),
                Migration18To19(),
                Migration19To20(),
                Migration21To22(),
                Migration23To24(),
                Migration24To25(),
                Migration26To27(),
                Migration28To29(),
                Migration29To30(),
                Migration35To36(),
                Migration38To39(),
                Migration48To49(),
                Migration55To56(),
                Migration57To58(),
                Migration60To61(),
                Migration62To63(),
                Migration63To64(),
                Migration65To66(),
                *chatMessageContentMigrations.toTypedArray() // 25 -> 26, 31 -> 32, 37 -> 38, 44 -> 45
            )
        }
    }

    abstract fun chainDao(): ChainDao

    abstract fun storageDao(): StorageDao

    abstract fun metaAccountDao(): MetaAccountDao

    abstract fun tokenBalanceDao(): TokenBalanceDao

    abstract fun chatMessageDao(): ChatMessageDao

    abstract fun chatMessageCompactionDao(): ChatMessageCompactionDao

    abstract fun contactDao(): ContactDao

    abstract fun contactDeviceDao(): ContactDeviceDao

    abstract fun tokenPriceDao(): TokenPriceDao

    abstract fun sendRecipientDao(): SendRecipientDao

    abstract fun chatMessageProcessingDao(): ChatMessageProcessingDao

    abstract fun processedChatMessageDao(): ProcessedChatMessageDao

    abstract fun coinageEntryDao(): CoinageEntryDao

    abstract fun messageReactionsDao(): ChatMessageReactionDao

    abstract fun videoGameVoteDao(): VideoGameVoteDao

    abstract fun vouchersDao(): VouchersDao

    abstract fun chatBotStateDao(): ChatBotStateDao

    abstract fun ssoSessionDao(): SsoSessionDao

    abstract fun messageRevisionDao(): MessageRevisionDao

    abstract fun gamePlayersDao(): GamePlayersDao

    abstract fun messageNotificationSentDao(): MessageNotificationSentDao

    abstract fun productDao(): ProductDao

    abstract fun browserTabDao(): BrowserTabDao

    abstract fun productFundingOperationDao(): ProductFundingOperationDao

    abstract fun productTopUpDao(): ProductTopUpDao

    abstract fun chatRequestDao(): ChatRequestDao

    abstract fun chatRequestSyncStateDao(): ChatRequestSyncStateDao

    abstract fun coinDao(): CoinDao

    abstract fun recyclerVoucherDao(): RecyclerVoucherDao

    abstract fun productPermissionGrantDao(): ProductPermissionGrantDao

    abstract fun videoGameBannedPlayerDao(): VideoGameBannedPlayerDao

    abstract fun productIntegrationDao(): ProductIntegrationDao

    abstract fun chatRoomDao(): ChatRoomDao

    abstract fun fileUploadDao(): FileUploadDao

    abstract fun fileDownloadDao(): FileDownloadDao

    abstract fun videoGameConnectionAttemptDao(): VideoGameConnectionAttemptDao

    abstract fun externalPaymentDao(): ExternalPaymentDao

    abstract fun statementStoreSlotAllocationDao(): StatementStoreSlotAllocationDao

    abstract fun scheduledProductNotificationDao(): ScheduledProductNotificationDao

    abstract fun trackedExtrinsicDao(): TrackedExtrinsicDao

    abstract fun removedChatDao(): RemovedChatDao

    abstract fun ssoHandledRequestDao(): SsoHandledRequestDao

    abstract fun chatDraftDao(): ChatDraftDao

    abstract fun chatSearchRecentDao(): ChatSearchRecentDao

    abstract fun ringVrfKeyRegistrationDao(): RingVrfKeyRegistrationDao

    abstract fun durableTxDao(): DurableTxDao

    abstract fun coinageInstallationDao(): CoinageInstallationDao

    abstract fun pocketCardDao(): PocketCardDao
}
