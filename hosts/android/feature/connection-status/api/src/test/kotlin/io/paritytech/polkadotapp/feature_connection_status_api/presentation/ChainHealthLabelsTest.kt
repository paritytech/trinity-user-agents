package io.paritytech.polkadotapp.feature_connection_status_api.presentation

import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthItemModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.IndicatorRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class ChainHealthLabelsTest {
    @Test
    fun `a chain that will not hold is broken, the statement store is disconnected`() {
        assertEquals(
            RCommon.string.chain_health_state_broken,
            item(IndicatorRow.People, ChainHealthIndicator.Disconnected).labelRes(),
        )
        assertEquals(
            RCommon.string.chain_health_state_disconnected,
            item(IndicatorRow.StatementStore, ChainHealthIndicator.Disconnected).labelRes(),
        )
    }

    @Test
    fun `every other state reads the same on both`() {
        val states = listOf(
            ChainHealthIndicator.Healthy(liveness = null) to RCommon.string.chain_health_state_connected,
            ChainHealthIndicator.Connecting to RCommon.string.chain_health_state_connecting,
            ChainHealthIndicator.Offline to RCommon.string.chain_health_state_offline,
        )

        states.forEach { (indicator, expected) ->
            assertEquals("$indicator", expected, item(IndicatorRow.People, indicator).labelRes())
            assertEquals("$indicator", expected, item(IndicatorRow.StatementStore, indicator).labelRes())
        }
    }

    @Test
    fun `only the chains carry the production line`() {
        assertTrue(IndicatorRow.People.measuresProduction())
        assertTrue(IndicatorRow.AssetHub.measuresProduction())
        assertTrue(IndicatorRow.Bulletin.measuresProduction())
        assertFalse(IndicatorRow.StatementStore.measuresProduction())
    }

    @Test
    fun `every row has its own name`() {
        val names = IndicatorRow.entries.map { it.nameRes() }

        assertEquals(names.size, names.toSet().size)
    }

    private fun item(row: IndicatorRow, indicator: ChainHealthIndicator) = ChainHealthItemModel(
        row = row,
        indicator = indicator,
    )
}
