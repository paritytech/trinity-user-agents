package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.common.utils.CollectionDiffer
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.TokenPriceLocal
import io.paritytech.polkadotapp.database.model.chain.ChainAssetLocal
import io.paritytech.polkadotapp.database.model.chain.ChainLocal
import io.paritytech.polkadotapp.database.model.chain.FullChainAssetIdLocal
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.math.BigDecimal

/**
 * Chain sync can remove an asset while price sync still holds the asset list it fetched prices for.
 * A price for a removed asset must not make the prices of the remaining assets fail to save.
 */
@RunWith(AndroidJUnit4::class)
class TokenPriceDaoTest {

    private lateinit var database: AppDatabase

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()
    }

    @After
    fun tearDown() = database.close()

    @Test
    fun aPriceForARemovedAssetDoesNotBlockThePricesOfExistingAssets() = runBlocking {
        givenChainWithAsset()

        dao().insert(
            listOf(
                priceOf(EXISTING_ASSET_ID, BigDecimal("1.5")),
                priceOf(REMOVED_ASSET_ID, BigDecimal("2.5")),
            )
        )

        assertEquals(listOf(priceOf(EXISTING_ASSET_ID, BigDecimal("1.5"))), dao().allPrices(CURRENCY_ID))
    }

    private fun dao() = database.tokenPriceDao()

    private suspend fun givenChainWithAsset() {
        database.chainDao().applyDiff(
            chainDiff = addedOnly(chain()),
            assetsDiff = addedOnly(asset(EXISTING_ASSET_ID)),
            nodesDiff = CollectionDiffer.Diff.empty(),
            explorersDiff = CollectionDiffer.Diff.empty(),
            externalApisDiff = CollectionDiffer.Diff.empty(),
        )
    }

    private fun <T> addedOnly(item: T) = CollectionDiffer.Diff(
        added = listOf(item),
        updated = emptyList(),
        removed = emptyList(),
        all = listOf(item),
    )

    private fun chain() = ChainLocal(
        id = CHAIN_ID,
        genesisHash = CHAIN_ID,
        parentId = null,
        name = "Test chain",
        types = null,
        prefix = 0,
        isEthereumBased = false,
        isTestNet = true,
        hasSubstrateRuntime = true,
        additional = null,
        connectionState = ChainLocal.ConnectionStateLocal.FULL_SYNC,
        nodeSelectionStrategy = ChainLocal.NodeSelectionStrategyLocal.ROUND_ROBIN,
    )

    private fun asset(id: Int) = ChainAssetLocal(
        id = id,
        chainId = CHAIN_ID,
        name = "Token",
        symbol = "TKN",
        priceId = "token",
        precision = 10,
        type = null,
        typeExtras = null,
        enabled = true,
    )

    private fun priceOf(assetId: Int, price: BigDecimal) = TokenPriceLocal(
        assetId = FullChainAssetIdLocal(chainId = CHAIN_ID, assetId = assetId),
        currencyId = CURRENCY_ID,
        price = price,
    )

    private companion object {
        const val CHAIN_ID = "test-chain"
        const val EXISTING_ASSET_ID = 0
        const val REMOVED_ASSET_ID = 1
        const val CURRENCY_ID = 0
    }
}
