package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.CoinageHolding
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The display order, which was wrong three times on iOS before it settled.
 *
 * Both layouts start a new block wherever the partition changes, so an unordered list produces a block — and
 * a header — per coin. That is what makes this worth its own test rather than being left to the eye.
 */
class CoinageBreakdownOrderTest {
    @Test
    fun `clearing leads, then largest denomination`() {
        val coins = CoinageBreakdownFactory.coins(
            listOf(
                holding(id = "small-ready", exponent = 2, isReady = true),
                holding(id = "large-ready", exponent = 12, isReady = true),
                holding(id = "small-clearing", exponent = 1, isReady = false),
                holding(id = "large-clearing", exponent = 9, isReady = false)
            )
        )

        assertEquals(
            listOf("large-clearing", "small-clearing", "large-ready", "small-ready"),
            coins.map { it.id }
        )
    }

    @Test
    fun `denomination outranks fungibility`() {
        val coins = CoinageBreakdownFactory.coins(
            listOf(
                // A penny nobody can trace used to lead the whole block, and the eye had nothing to hold on to.
                holding(id = "penny-untraceable", exponent = 0, fungibility = 0),
                holding(id = "large-fungible", exponent = 14, fungibility = 100)
            )
        )

        assertEquals(listOf("large-fungible", "penny-untraceable"), coins.map { it.id })
    }

    @Test
    fun `within a denomination the least fungible leads, then the deepest history`() {
        val coins = CoinageBreakdownFactory.coins(
            listOf(
                holding(id = "fungible", exponent = 6, fungibility = 100),
                holding(id = "hidden-nowhere", exponent = 6, fungibility = 0),
                holding(id = "no-record", exponent = 6, fungibility = null),
                holding(id = "fungible-well-travelled", exponent = 6, fungibility = 100, hops = 5)
            )
        )

        // No record and a score of zero both sit at level zero; the deepest history breaks that tie, and a
        // well-travelled coin at full fungibility still comes after both.
        assertEquals(
            listOf("no-record", "hidden-nowhere", "fungible-well-travelled", "fungible"),
            coins.map { it.id }
        )
    }

    @Test
    fun `ordering is total over coins that never had a holding behind them`() {
        val scrambled = listOf(
            coin(id = "ready-small", exponent = 1, partition = CoinageStripLayout.Partition.READY),
            coin(id = "clearing-small", exponent = 1, partition = CoinageStripLayout.Partition.CLEARING),
            coin(id = "ready-large", exponent = 10, partition = CoinageStripLayout.Partition.READY),
            coin(id = "clearing-large", exponent = 10, partition = CoinageStripLayout.Partition.CLEARING)
        )

        assertEquals(
            listOf("clearing-large", "clearing-small", "ready-large", "ready-small"),
            CoinageBreakdownFactory.inDisplayOrder(scrambled).map { it.id }
        )
    }

    @Test
    fun `a batch unload wears two levels harder than the same score alone`() {
        val alone = CoinageBreakdownFactory.wear(RecyclerFungibility.ofPercent(100), isBatchUnloaded = false)
        val batched = CoinageBreakdownFactory.wear(RecyclerFungibility.ofPercent(100), isBatchUnloaded = true)

        assertEquals(CoinageWear.maximumLevel, CoinageWear.level(alone))
        assertEquals(CoinageWear.maximumLevel - 2, CoinageWear.level(batched))
    }

    @Test
    fun `no recycler record wears as though it hides among nobody`() {
        assertEquals(
            CoinageWear.amount(0),
            CoinageBreakdownFactory.wear(score = null, isBatchUnloaded = false)
        )
    }

    private fun holding(
        id: String,
        exponent: Int,
        isReady: Boolean = true,
        fungibility: Int? = 50,
        hops: Int = 0
    ) = CoinageHolding(
        id = id,
        exponent = ValueExponent(exponent),
        derivationIndex = CoinageKeyIndex(INSTALLATION, id.hashCode()),
        isReady = isReady,
        recyclerFungibility = fungibility?.let(RecyclerFungibility::ofPercent),
        isBatchUnloaded = false,
        hops = hops
    )

    private fun coin(id: String, exponent: Int, partition: CoinageStripLayout.Partition) = CoinageScene.Coin(
        id = id,
        exponent = exponent,
        wear = 0f,
        partition = partition,
        level = CoinageWear.maximumLevel,
        hops = 0
    )

    private companion object {
        val INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES).toDataByteArray())
    }
}
