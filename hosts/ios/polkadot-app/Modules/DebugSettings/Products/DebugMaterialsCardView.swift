import PolkadotUI
import SwiftUI
import UIKit

/// A chat card wrapped in one `Materials` node, drawn the way a message card is, with no product
/// in the way. A plain line under it shows where the card ends.
struct DebugMaterialsCardView: View {
    @State var viewModel: DebugMaterialsCardViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ProductWidgetChatView(
                messageId: DebugMaterialsCardViewModel.messageId,
                nodeProvider: viewModel,
                onAction: nil,
                resolveImage: WidgetImageResolver { _ in UIImage.debugPicture() },
                materials: viewModel.materials
            )
            Text("A plain line under the card")
            if let refusal = viewModel.refusal {
                Text(refusal).foregroundStyle(.red)
            }
            Spacer()
        }
        .padding(16)
        .task { await viewModel.load() }
    }
}

private extension UIImage {
    /// A green square with a yellow disc, colours the material's blue and red do not come near.
    static func debugPicture() -> UIImage {
        let bounds = CGRect(x: 0, y: 0, width: 48, height: 48)
        return UIGraphicsImageRenderer(bounds: bounds).image { context in
            UIColor.systemGreen.setFill()
            context.fill(bounds)
            UIColor.systemYellow.setFill()
            context.cgContext.fillEllipse(in: bounds.insetBy(dx: 10, dy: 10))
        }
    }
}
