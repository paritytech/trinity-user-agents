package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy.Companion.REPLACE
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.BrowserTabLocal

@Dao
interface BrowserTabDao {
    @Query("SELECT * FROM browser_tabs ORDER BY position")
    suspend fun getAll(): List<BrowserTabLocal>

    @Insert(onConflict = REPLACE)
    suspend fun upsert(tab: BrowserTabLocal)

    @Query("DELETE FROM browser_tabs WHERE id = :id")
    suspend fun deleteById(id: Long)
}
