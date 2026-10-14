package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration1to2
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration2to3
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration3to4
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration4to5

@Module
@InstallIn(SingletonComponent::class)
internal interface ChatContentMigrationsModule {
    companion object {
        @Provides
        @IntoSet
        fun provideMigration1to2(): ChatMessageContentMigration<*, *> = createChatMessageMigration1to2()

        @Provides
        @IntoSet
        fun provideMigration2to3(): ChatMessageContentMigration<*, *> = createChatMessageMigration2to3()

        @Provides
        @IntoSet
        fun provideMigration3to4(): ChatMessageContentMigration<*, *> = createChatMessageMigration3to4()

        @Provides
        @IntoSet
        fun provideMigration4to5(): ChatMessageContentMigration<*, *> = createChatMessageMigration4to5()
    }
}
