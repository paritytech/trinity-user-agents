package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.paritytech.polkadotapp.database.model.TokenPriceLocal
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class TokenPriceDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    abstract suspend fun insert(prices: List<TokenPriceLocal>)

    @Query("SELECT * FROM tokenPrices WHERE chainId = :chainId AND assetId = :assetId AND currencyId = :currencyId")
    abstract suspend fun getPrice(chainId: String, assetId: Int, currencyId: Int): TokenPriceLocal?

    @Query("SELECT * FROM tokenPrices WHERE chainId = :chainId AND assetId = :assetId AND currencyId = :currencyId")
    abstract fun priceFlow(chainId: String, assetId: Int, currencyId: Int): Flow<TokenPriceLocal?>

    @Query("SELECT * FROM tokenPrices WHERE currencyId = :currencyId")
    abstract suspend fun allPrices(currencyId: Int): List<TokenPriceLocal>

    suspend fun getPrices(currencyId: Int, assetIds: Collection<FullChainAssetIdLocal>): List<TokenPriceLocal> {
        val assetIdsConcat = assetIds.map { "${it.chainId}_${it.assetId}" }
        return getPricesByConcatIds(currencyId, assetIdsConcat)
    }

    @Query("SELECT * FROM tokenPrices WHERE currencyId = :currencyId AND chainId || '_' || assetId IN (:assetIdConcat)")
    protected abstract suspend fun getPricesByConcatIds(currencyId: Int, assetIdConcat: Collection<String>): List<TokenPriceLocal>

    @Query("SELECT * FROM tokenPrices WHERE currencyId = :currencyId")
    abstract fun allPricesFlow(currencyId: Int): Flow<List<TokenPriceLocal>>
}
