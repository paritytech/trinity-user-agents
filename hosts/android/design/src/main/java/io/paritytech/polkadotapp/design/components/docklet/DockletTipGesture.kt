package io.paritytech.polkadotapp.design.components.docklet

object DockletTipGesture {
    private const val FlingAway = 56f

    fun flungAway(movedDp: Float): Boolean = movedDp > FlingAway
}
