import PolkadotUI
import SwiftUI

/// The frame the host draws a Pocket card in, on the Wallet tab and in the
/// approval sheet alike, so a face is authored against one size.
enum PocketCardSize {
    static let height: CGFloat = 234
    static let cornerRadius: CGFloat = 24
}

/// One product-backed card, drawn from the face the product supplied.
///
/// The host owns the frame, the ground it sits on and the corner radius; the
/// product owns everything inside.
struct PocketProductCardView: View {
    let title: String
    let face: CustomMessageWidgetNode?
    /// How the images inside the face are read. Required rather than
    /// defaulted: a caller that leaves it out draws the face with blank space
    /// where its images are, and nothing about the card says so.
    let resolveImage: WidgetImageResolver?
    var onAction: WidgetActionHandler?

    var body: some View {
        ZStack(alignment: .topLeading) {
            RoundedRectangle(cornerRadius: PocketCardSize.cornerRadius)
                .fill(Color(.bgSurfaceContainer))

            if let face {
                // The face is authored against the whole card, so it is handed
                // the full frame: its own modifiers decide where things sit,
                // and a centring container here would override them.
                CustomMessageWidgetView(node: face, onAction: onAction, resolveImage: resolveImage)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                // A card whose product has not drawn yet keeps its place rather
                // than collapsing the layout around it, and says it is still
                // coming: a title on its own reads as the card the product meant
                // to draw.
                VStack(alignment: .leading, spacing: 12) {
                    Text(title)
                        .textStyle(.headline16Medium())
                        .foregroundStyle(Color(.fgSecondary))
                    ProgressView()
                }
                .padding(16)
            }
        }
        .frame(maxWidth: .infinity)
        .frame(height: PocketCardSize.height)
        .clipShape(RoundedRectangle(cornerRadius: PocketCardSize.cornerRadius))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
    }
}
