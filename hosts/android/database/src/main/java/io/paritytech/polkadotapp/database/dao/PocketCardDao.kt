package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.PocketCardFaceLocal
import io.paritytech.polkadotapp.database.model.PocketCardLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface PocketCardDao {
    @Query("SELECT * FROM pocket_cards")
    fun observeAll(): Flow<List<PocketCardLocal>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(card: PocketCardLocal)

    @Query("DELETE FROM pocket_cards WHERE productId = :productId AND cardId = :cardId")
    suspend fun delete(productId: String, cardId: String): Int

    @Query("SELECT face FROM pocket_card_faces WHERE productId = :productId AND cardId = :cardId")
    suspend fun getFace(productId: String, cardId: String): ByteArray?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertFace(face: PocketCardFaceLocal)

    @Query("DELETE FROM pocket_card_faces WHERE productId = :productId AND cardId = :cardId")
    suspend fun deleteFace(productId: String, cardId: String)
}
