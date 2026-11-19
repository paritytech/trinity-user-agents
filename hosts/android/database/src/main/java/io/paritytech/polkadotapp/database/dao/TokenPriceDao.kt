package io.paritytech.polkadotapp.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import io.paritytech.polkadotapp.database.model.TokenPriceLocal
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import kotlinx.coroutines.flow.Flow

@Dao
abstract class TokenPriceDao {
    /**
     * Skips the prices of assets that are no longer in `chain_assets`: chain sync can remove an asset
     * between the read of the asset list and this insert, and one missing parent would fail the whole batch.
     */
    @Transaction
    open suspend fun insert(prices: List<TokenPriceLocal>) {
        val existingAssetIds = getAllAssetIds().toHashSet()
        insertAll(prices.filter { it.assetId in existingAssetIds })
    }

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    protected abstract suspend fun insertAll(prices: List<TokenPriceLocal>)

    @Query("SELECT chainId, id AS assetId FROM chain_assets")
    protected abstract suspend fun getAllAssetIds(): List<FullChainAssetIdLocal>

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
