package io.paritytech.polkadotapp.app.root.navigation.scan

import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.feature_scan_impl.ScanRouter
import javax.inject.Inject

class ScanNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), ScanRouter
