package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "browser_tabs")
class BrowserTabLocal(
    @PrimaryKey val id: Long,
    val url: String,
    val title: String,
    val position: Int,
    val lastActive: Long,
)
