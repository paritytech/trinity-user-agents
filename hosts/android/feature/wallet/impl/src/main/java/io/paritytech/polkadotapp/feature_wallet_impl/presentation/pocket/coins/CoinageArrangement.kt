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
        /** What the view needs to be tall enough for, the band below the coins included. */
        val height: Float,
        /** Where each partition's header sits, for the labels over the grid. */
        val blocks: List<CoinageGridLayout.Block>,
        /** How far each partition's run reaches across the strip, for the rule under it. */
        val runs: List<CoinageStripLayout.Span>,
        /** Where the band's top sits while a coin is picked, for the detail beside the coin. */
        val focusTop: Float?,
        /** How tall the coins alone are, before any band below them. */
        val coinsHeight: Float
    )

    /**
     * One coin picked out of the grid by a tap: drawn large at the left of a band, its detail beside it, and
     * every other coin faded back where it is.
     */
    data class Focus(
        val id: String,
        /** How tall the band is: never less than the coin, taller if the detail needs it. */
        val band: Float,
        /** The part of the field on screen and clear of the bars, in field coordinates. */
        val visibleTop: Float,
        val visibleBottom: Float
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

        /** A face the figure can be read on, well past the sixty points the strip allows. */
        const val FOCUS_DIAMETER = 112f
        const val FOCUS_INSET = 8f

        /** Clear of the grid, which it flies up out of and back into. */
        const val FOCUS_LIFT = GRID_LIFT + 60f

        /** The least a band can be: the picked coin with an inset above and below. */
        const val FOCUS_BAND_MINIMUM = FOCUS_DIAMETER + FOCUS_INSET * 2

        /** The thickness `designs.json` carries for a coin whose row is missing. */
        private const val FALLBACK_THICKNESS = 0.075f

        fun targets(
            coins: List<CoinageScene.Coin>,
            arrangement: CoinageArrangement,
            areaWidth: Float,
            areaHeight: Float,
            designs: List<CoinageAssetStore.Design>,
            stripHeight: Float,
            focus: Focus?
        ): Result {
            val laid = when (arrangement) {
                STRIP -> strip(coins, areaWidth, designs, stripHeight)
                GRID -> grid(coins, areaWidth, areaHeight)
            }

            if (focus == null || coins.none { it.id == focus.id }) return laid

            return picked(focus, laid)
        }

        /**
         * Where the band's top goes for what is on screen now: below the coins when there is room for it
         * there, so a short grid is never covered; otherwise floating at the bottom of the visible area, so
         * the picked coin and its detail are in reach however far down a tall grid the page is. Asked again
         * on every scroll.
         */
        fun bandTop(focus: Focus, coinsHeight: Float): Float {
            val band = maxOf(focus.band, FOCUS_BAND_MINIMUM)
            val bottom = focus.visibleBottom - FOCUS_INSET

            return if (coinsHeight + band <= bottom) coinsHeight else maxOf(bottom - band, focus.visibleTop)
        }

        fun pickedCentreX(): Float = FOCUS_INSET + FOCUS_DIAMETER / 2

        fun pickedCentreY(bandTop: Float): Float = bandTop + FOCUS_BAND_MINIMUM / 2

        /** A band below the coins grows the field; one floating over them does not. */
        fun fieldHeight(focus: Focus, coinsHeight: Float, bandTop: Float): Float =
            maxOf(coinsHeight, bandTop + maxOf(focus.band, FOCUS_BAND_MINIMUM))

        private fun picked(focus: Focus, laid: Result): Result {
            val top = bandTop(focus, laid.height)
            val targets = laid.targets.map { (coin, target) ->
                coin to if (coin.id == focus.id) {
                    target.copy(
                        centreX = pickedCentreX(),
                        centreY = pickedCentreY(top),
                        height = FOCUS_DIAMETER,
                        turn = 0f,
                        tilt = 0f,
                        thickness = 1f,
                        luster = 1f,
                        calm = 0f,
                        lift = FOCUS_LIFT,
                        recede = 0f
                    )
                } else {
                    target.copy(recede = 1f)
                }
            }

            return laid.copy(
                targets = targets,
                height = fieldHeight(focus, laid.height, top),
                focusTop = top,
                coinsHeight = laid.height
            )
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
                    // Not calmed as the reference calms its edge-on coins: calming swaps the normal for a plain
                    // radial one and fades the reeding, so a turned coin read as a flat band. The outlines and
                    // the milling are the point of the rim, and wear has to read on it.
                    calm = 0f
                )
            }

            return Result(
                targets = targets,
                height = height,
                blocks = emptyList(),
                runs = layout.partitions,
                focusTop = null,
                coinsHeight = height
            )
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
                runs = emptyList(),
                focusTop = null,
                coinsHeight = layout.height
            )
        }
    }
}
