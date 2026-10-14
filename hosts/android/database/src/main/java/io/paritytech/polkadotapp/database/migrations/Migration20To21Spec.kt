package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteTable
import androidx.room.migration.AutoMigrationSpec

@DeleteTable(tableName = "chat_payment_detection")
class Migration20To21Spec : AutoMigrationSpec
