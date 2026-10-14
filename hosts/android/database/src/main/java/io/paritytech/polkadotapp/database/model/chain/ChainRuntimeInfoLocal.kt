package io.paritytech.polkadotapp.database.model.chain

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "chain_runtimes",
    primaryKeys = ["chainId"],
    indices = [
        Index(value = ["chainId"])
    ],
    foreignKeys = [
        ForeignKey(
            entity = ChainLocal::class,
            parentColumns = ["id"],
            childColumns = ["chainId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
class ChainRuntimeInfoLocal(
    val chainId: String,
    val syncedVersion: Int,
    val syncedTransactionVersion: Int?,
    val remoteVersion: Int,
    val transactionVersion: Int?,
    // Pre-existing rows default to 1 so that RuntimeCacheMigrator forces a single metadata re-fetch on upgrade
    @ColumnInfo(defaultValue = "1") val localMigratorVersion: Int,
)
