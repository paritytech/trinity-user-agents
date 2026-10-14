package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.TokenBalanceLocal
import kotlinx.coroutines.flow.Flow

@Dao
interface TokenBalanceDao {
    @Query("SELECT * FROM tokenBalances WHERE metaId = :metaId AND chainId = :chainId AND assetId = :assetId")
    fun observeAsset(metaId: Long, chainId: String, assetId: Int): Flow<TokenBalanceLocal?>

    @Query("SELECT * FROM tokenBalances WHERE metaId = :metaId AND chainId = :chainId AND assetId = :assetId")
    suspend fun getAsset(metaId: Long, chainId: String, assetId: Int): TokenBalanceLocal?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAsset(asset: TokenBalanceLocal)
}
