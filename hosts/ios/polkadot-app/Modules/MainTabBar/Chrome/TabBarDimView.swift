import SwiftUI
import PolkadotUI

/// Dims the whole screen behind the open centre panel. It reads the same `TabBarBarViewModel.centerProgress` as the
/// bar, so it follows the morph in the same transaction. The bar's host is too narrow and starts below the top safe
/// area to carry the dim itself.
struct TabBarDimView: View {
    let model: TabBarBarViewModel

    var body: some View {
        DSTabBarDimLayer()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .opacity(model.centerProgress)
            .ignoresSafeArea()
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}
