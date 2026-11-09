package io.paritytech.polkadotapp.design.components.docklet

data class DockletAutoHideBlockers(
    val shownAtRest: Boolean,
    val searchOpen: Boolean,
    val keyboardUp: Boolean,
    val tipShown: Boolean,
    val panelExpanded: Boolean,
    val touchExploration: Boolean,
) {
    val blocked: Boolean get() = !shownAtRest || searchOpen || keyboardUp || tipShown || panelExpanded || touchExploration
}
