package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

/**
 * Where every coin is heading, for one of the two arrangements.
 *
 * The strip and the grid are the same coins in different places, so this hands the field a set of targets
 * and nothing else changes. A toggle between them is a retarget, which is why the coins fly rather than cut.
 */
enum class CoinageArrangement {
    STRIP,
    GRID;

    data class Result(
        val targets: List<Pair<CoinageScene.Coin, CoinageCoinField.Target>>,
        /** What the view needs to be tall enough for. */
        val height: Float,
        /** Where each partition's header sits, for the labels over the grid. */
        val blocks: List<CoinageGridLayout.Block>,
        /** How far each partition's run reaches across the strip, for the rule under it. */
        val runs: List<CoinageStripLayout.Span>
    )

    companion object {
        /**
         * How far in front of the strip the grid sits.
         *
         * A coin unturning from edge-on swings half of itself away from the viewer, so a coin that has
         * started moving used to cut through the neighbours still standing edge-on beside it. Carrying it
         * forward as it turns keeps it clear of them, and leaves the grid reading as the nearer of the two.
         *
         * It costs nothing to look at: the projection is orthographic, so depth orders coins without making
         * the nearer ones larger.
         */
        const val GRID_LIFT = 40f

        /** The thickness `designs.json` carries for a coin whose row is missing. */
        private const val FALLBACK_THICKNESS = 0.075f

        fun targets(
            coins: List<CoinageScene.Coin>,
            arrangement: CoinageArrangement,
            areaWidth: Float,
            areaHeight: Float,
            designs: List<CoinageAssetStore.Design>,
            stripHeight: Float
        ): Result = when (arrangement) {
            STRIP -> strip(coins, areaWidth, designs, stripHeight)
            GRID -> grid(coins, areaWidth, areaHeight)
        }

        private fun design(coin: CoinageScene.Coin, designs: List<CoinageAssetStore.Design>) =
            designs.getOrNull(coin.exponent.coerceIn(0, maxOf(designs.lastIndex, 0)))

        private fun thickness(coin: CoinageScene.Coin, designs: List<CoinageAssetStore.Design>) =
            design(coin, designs)?.thickness ?: FALLBACK_THICKNESS

        private fun strip(
            coins: List<CoinageScene.Coin>,
            width: Float,
            designs: List<CoinageAssetStore.Design>,
            height: Float
        ): Result {
            val layout = CoinageStripLayout.layout(
                coins.map { coin ->
                    CoinageStripLayout.Coin(
                        size = CoinageCoinDesign.design(coin.exponent).size,
                        width = CoinageScene.geometry(coin.exponent).faceWidth,
                        thickness = thickness(coin, designs),
                        partition = coin.partition
                    )
                },
                width = width,
                options = CoinageStripLayout.Options(height = height)
            )

            val targets = coins.zip(layout.coins) { coin, placement ->
                coin to CoinageCoinField.Target(
                    centreX = placement.centreX,
                    centreY = placement.centreY,
                    height = placement.height,
                    turn = placement.turn,
                    thickness = placement.thickness / thickness(coin, designs),
                    wear = coin.wear,
                    // The strip skips the six-tap luster path; the grid does not.
                    luster = 0f,
                    calm = CoinageStripLayout.edgeCalm(placement.turn)
                )
            }

            return Result(targets = targets, height = height, blocks = emptyList(), runs = layout.partitions)
        }

        private fun grid(coins: List<CoinageScene.Coin>, areaWidth: Float, areaHeight: Float): Result {
            val byId = coins.associateBy { it.id }
            val layout = CoinageGridLayout.layout(
                coins.map { CoinageGridLayout.Item(it.id, it.exponent, it.partition) },
                areaWidth = areaWidth,
                areaHeight = areaHeight
            )

            val targets = layout.cells.mapNotNull { cell ->
                val coin = byId[cell.id] ?: return@mapNotNull null

                coin to CoinageCoinField.Target(
                    centreX = cell.centreX,
                    centreY = cell.centreY,
                    height = layout.diameter * CoinageCoinDesign.design(coin.exponent).size,
                    turn = 0f,
                    thickness = 1f,
                    wear = coin.wear,
                    luster = 1f,
                    calm = 0f,
                    lift = GRID_LIFT
                )
            }

            return Result(
                targets = targets,
                height = layout.height,
                blocks = layout.blocks,
                // The grid labels its blocks with headers of its own.
                runs = emptyList()
            )
        }
    }
}
