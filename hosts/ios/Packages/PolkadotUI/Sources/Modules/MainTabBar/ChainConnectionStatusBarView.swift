import SwiftUI
import UIKitExt

public struct ChainConnectionStatusBarView: View, Hashable {
    // Height of the bar the container reserves via `additionalSafeAreaInsets.top`.
    public static let preferredHeight: CGFloat = 20

    /// Ring geometry, public so a UIKit anchor can be sized to the cluster the strip lays out.
    static let ringDiameter: CGFloat = 20

    static let ringSpacing: CGFloat = 6

    /// Width the trailing ring cluster occupies for `count` rings, excluding horizontal padding.
    public static func ringsWidth(count: Int) -> CGFloat {
        guard count > 0 else {
            return 0
        }

        return CGFloat(count) * ringDiameter + CGFloat(count - 1) * ringSpacing
    }

    private static let bottomSpacing: CGFloat = 8

    private static var bottomPadding: CGFloat {
        guard #available(iOS 26.0, *) else { return 0 }

        let topInset = UIWindow.keyWindow?.safeAreaInsets.top ?? 0
        let surplus = max(0, topInset - preferredHeight)

        return surplus > bottomSpacing ? bottomSpacing : 0
    }

    private static var horizontalPadding: CGFloat {
        guard #available(iOS 26.0, *) else { return 6 }

        let topInset = UIWindow.keyWindow?.safeAreaInsets.top ?? 0
        return topInset > 0 ? 16 : 6
    }

    public let models: [ChainConnectionStatusViewModel]

    public init(models: [ChainConnectionStatusViewModel]) {
        self.models = models
    }

    public var body: some View {
        HStack(spacing: Self.ringSpacing) {
            Spacer(minLength: 0)
            ForEach(models) { viewModel in
                ChainStatusRingView(viewModel: viewModel, diameter: Self.ringDiameter)
            }
        }
        .safeAreaPadding(.horizontal, Self.horizontalPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: Self.preferredHeight)
        // Grows the view's intrinsic height upward into the unused status-bar
        // headroom. Deliberately not part of `preferredHeight`.
        .padding(.bottom, Self.bottomPadding)
    }
}
