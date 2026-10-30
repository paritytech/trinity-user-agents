package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteTable
import androidx.room.migration.AutoMigrationSpec

/** Drops the host's funding history: the core keeps every ended session and the host reads it from there. */
@DeleteTable(tableName = "product_funding_records")
class Migration73To74Spec : AutoMigrationSpec
