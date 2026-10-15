package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.debug

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageBreakdownFactory
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageCoinDesign
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageCoinTrace
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageScene
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageStripLayout
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageWear
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import java.math.BigDecimal
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Which set of holdings the coins draw. Scaffolding for reviewing the depiction against wallets the app cannot
 * easily be put into, shown with the other debug features only. Delete this file to remove it.
 */
internal enum class CoinageTestDataMode(val title: Int) {
    LIVE(RCommon.string.pocket_debug_coinage_test_live),
    EVERY_DENOMINATION(RCommon.string.pocket_debug_coinage_test_values),
    EVERY_STATUS(RCommon.string.pocket_debug_coinage_test_states),
    RANDOM_FIFTY(RCommon.string.pocket_debug_coinage_test_fifty),
    RANDOM_FIVE_HUNDRED(RCommon.string.pocket_debug_coinage_test_five_hundred);

    /**
     * The state with its coins and their details swapped for the test set, or untouched for [LIVE]. The figures
     * above the coins stay the wallet's own.
     */
    fun applied(state: CoinageUiState.TokensState): CoinageUiState.TokensState {
        val coins = when (this) {
            LIVE -> return state
            EVERY_DENOMINATION -> CoinageTestData.everyDenomination()
            EVERY_STATUS -> CoinageTestData.everyStatus()
            RANDOM_FIFTY -> CoinageTestData.random(count = 50, seed = 50)
            RANDOM_FIVE_HUNDRED -> CoinageTestData.random(count = 500, seed = 500)
        }.let(CoinageBreakdownFactory::inDisplayOrder)

        return state.copy(
            coins = coins.toImmutableList(),
            traces = CoinageTestData.traces(coins).toImmutableMap()
        )
    }
}

@Composable
internal fun CoinageTestDataSwitch(
    modifier: Modifier = Modifier,
    mode: CoinageTestDataMode,
    onModeChange: (CoinageTestDataMode) -> Unit
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        CoinageTestDataMode.entries.forEach { option ->
            val selected = option == mode

            NovaText(
                modifier = Modifier.selectable(
                    selected = selected,
                    role = Role.RadioButton,
                    onClick = { onModeChange(option) }
                ),
                text = (if (selected) "● " else "○ ") + stringResource(option.title),
                maxLines = 1,
                style = PolkadotTheme.typography.body.small,
                color = if (selected) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary
            )
        }
    }
}

internal object CoinageTestData {
    private const val STATUS_EXPONENT = 4
    private const val STATUS_COUNT = 20

    /** One of every denomination, all at the best fungibility, so the fifteen designs show with nothing on them. */
    fun everyDenomination(): List<CoinageScene.Coin> = (0..CoinageCoinDesign.HIGHEST_EXPONENT).map { exponent ->
        coin(id = "test-value-$exponent", exponent = exponent, level = CoinageWear.maximumLevel)
    }

    /**
     * Twenty holdings of one denomination across the whole wear ladder. A single value keeps metal and outline
     * fixed, so the only thing changing is how traceable the holding is.
     */
    fun everyStatus(): List<CoinageScene.Coin> = (0 until STATUS_COUNT).map { index ->
        coin(
            id = "test-state-$index",
            exponent = STATUS_EXPONENT,
            level = CoinageWear.maximumLevel - index * CoinageWear.maximumLevel / (STATUS_COUNT - 1),
            isReady = index % 4 != 0,
            hops = index % 5
        )
    }

    /** A wallet of no particular shape, seeded, so a given size always draws the same wallet. */
    fun random(count: Int, seed: Int): List<CoinageScene.Coin> {
        val noise = CoinageNoise("test-random-$seed")
        val exponents = CoinageCoinDesign.HIGHEST_EXPONENT + 1

        return (0 until count).map { index ->
            val exponent = minOf((noise.next() * exponents).toInt(), CoinageCoinDesign.HIGHEST_EXPONENT)
            val level = minOf((noise.next() * (CoinageWear.maximumLevel + 1)).toInt(), CoinageWear.maximumLevel)

            coin(
                id = "test-random-$seed-$index",
                exponent = exponent,
                level = level,
                isReady = noise.next() > 0.25,
                hops = (noise.next() * 5).toInt()
            )
        }
    }

    /** A detail per test coin, so tapping one shows what the live card would. */
    fun traces(coins: List<CoinageScene.Coin>): Map<String, CoinageCoinTrace> = coins.associate { coin ->
        coin.id to CoinageCoinTrace(
            value = TestAmount(CENT.multiply(BigDecimal(2).pow(coin.exponent))),
            partition = coin.partition,
            hops = coin.hops,
            quality = CoinageCoinTrace.Quality.of(coin.level * 100 / CoinageWear.maximumLevel)
        )
    }

    private fun coin(id: String, exponent: Int, level: Int, isReady: Boolean = true, hops: Int = 0) =
        CoinageScene.Coin(
            id = id,
            exponent = exponent,
            wear = CoinageWear.amount(level),
            partition = if (isReady) CoinageStripLayout.Partition.READY else CoinageStripLayout.Partition.CLEARING,
            level = level,
            hops = hops
        )

    private val CENT = BigDecimal("0.01")

    private class TestAmount(override val amount: BigDecimal) : TokenAmountModel {
        override val appearance = TokenSymbolAppearance.DigitalDollar
    }
}

/** Deterministic scatter (xorshift), so a given test set draws the same wallet every time. */
private class CoinageNoise(seed: String) {
    private var state: ULong = seed.fold(0x9E3779B97F4A7C15uL) { hashed, character ->
        hashed * 31uL + character.code.toULong()
    } or 1uL

    fun next(): Double {
        state = state xor (state shl 13)
        state = state xor (state shr 7)
        state = state xor (state shl 17)

        return (state % 100_000uL).toDouble() / 100_000
    }
}
