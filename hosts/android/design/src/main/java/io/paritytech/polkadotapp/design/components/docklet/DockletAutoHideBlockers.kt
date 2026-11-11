package io.paritytech.polkadotapp.design.components.docklet

class DockletAutoHideBlockers(
    val shownAtRest: Boolean,
    val searchOpen: Boolean,
    val keyboardUp: Boolean,
    val tipShown: Boolean,
    val panelExpanded: Boolean,
    val touchExploration: Boolean,
    val resumed: Boolean,
) {
    val blocked: Boolean get() = !shownAtRest || !resumed || searchOpen || keyboardUp || tipShown || panelExpanded || touchExploration
}
