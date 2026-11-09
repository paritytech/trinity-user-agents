package io.paritytech.polkadotapp.design.components.docklet

enum class DockletBackEdge { Left, Right, None }

enum class DockletBackAction { Claim, Restore, PlainBack }

fun dockletBackAction(edge: DockletBackEdge, touchY: Float, band: ClosedFloatingPointRange<Float>): DockletBackAction = when {
    edge == DockletBackEdge.None -> DockletBackAction.Restore
    edge == DockletBackEdge.Right && touchY in band -> DockletBackAction.Claim
    else -> DockletBackAction.PlainBack
}

// Before Android 15 a Back button press reports the left edge, so only a swipe there is trusted to be one.
fun dockletBackEdge(swipeEdge: Int, leftEdge: Int, rightEdge: Int, sdkInt: Int): DockletBackEdge = when (swipeEdge) {
    rightEdge -> DockletBackEdge.Right
    leftEdge -> if (sdkInt >= EdgeNoneSdk) DockletBackEdge.Left else DockletBackEdge.None
    else -> DockletBackEdge.None
}

fun dockletSearchBackStep(step: DockletStep): DockletStep = if (step == DockletStep.Step2) DockletStep.Step1 else DockletStep.Rest

private const val EdgeNoneSdk = 35
