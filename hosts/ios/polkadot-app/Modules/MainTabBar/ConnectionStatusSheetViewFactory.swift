import DesignSystem
import PolkadotUI
import SwiftUI
import UIKit

@MainActor
enum ConnectionStatusSheetViewFactory {
    static func createView(viewModel: ConnectionStatusSheetViewModel) -> UIViewController {
        let controller = UIHostingController(rootView: ConnectionStatusSheetView(viewModel: viewModel))
        controller.view.backgroundColor = .bgSurfaceMain
        controller.modalPresentationStyle = .pageSheet
        controller.sheetPresentationController?.detents = [.medium()]
        controller.sheetPresentationController?.prefersGrabberVisible = true
        return controller
    }
}

/// Reads `rows` inside `body` so observation tracks it; handing the rows to the hosting controller
/// directly would snapshot them once and the sheet would never update. The scroll view keeps a long
/// chain list reachable at the `.medium()` detent, where the sheet does not grow to fit its content.
private struct ConnectionStatusSheetView: View {
    let viewModel: ConnectionStatusSheetViewModel

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            ConnectionStatusPanelView(rows: viewModel.rows)
        }
    }
}
