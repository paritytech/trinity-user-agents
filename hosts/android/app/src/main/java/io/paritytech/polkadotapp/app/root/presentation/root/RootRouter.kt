package io.paritytech.polkadotapp.app.root.presentation.root

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload

interface RootRouter : ReturnableRouter {
    fun openClaimUsername()

    fun openMain()

    fun openActiveProduct()

    fun openDebugMenu()

    fun openIssueReport(screenshotPath: String)

    fun openProductBotsManagement()

    /** Debug: draw a renderer tree served over HTTP at Pocket card size. */
    fun openPocketFacePreview()

    fun openSpaBrowser(payload: SpaBrowserPayload)
}
