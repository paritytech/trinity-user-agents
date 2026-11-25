package io.paritytech.polkadotapp.design.components.docklet

enum class DockletTouch { Docklet, Outside }

fun dockletTouch(point: DockletPoint, areas: List<DockletFrame>): DockletTouch =
    if (areas.any { point.x in it.left.value..it.right.value && point.y in it.top.value..it.bottom.value }) {
        DockletTouch.Docklet
    } else {
        DockletTouch.Outside
    }
