import PolkadotUI
import SwiftUI
import UIKit

/// The one card while it shrinks into the header: drawn over the wallet, in
/// window coordinates, from where the page's card rests to the centre of the
/// navigation bar, where it stays as a thumbnail however far the page scrolls.
struct PurseFloatingCard: View {
    let card: AssetDetailsBalanceCard.ViewModel
    let isUpdating: Bool
    /// The page's card before any scroll, in the window.
    let rest: CGRect
    let scrollOffset: CGFloat
    /// The navigation bar's centre, in the window.
    let dock: CGPoint

    var body: some View {
        GeometryReader { proxy in
            let origin = proxy.frame(in: .global).origin
            let frame = PurseMotion.Collapse.floatingFrame(rest: rest, scrollOffset: scrollOffset, dock: dock)
            AssetDetailsBalanceCard(
                viewModel: card,
                isUpdating: isUpdating,
                isExpanded: true,
                balanceOpacity: PurseMotion.Collapse.balanceOpacity(
                    PurseMotion.Collapse.progress(scrollOffset: scrollOffset)
                )
            )
            .frame(width: rest.width, height: rest.height)
            .scaleEffect(rest.width > 0 ? frame.width / rest.width : 1)
            .position(x: frame.midX - origin.x, y: frame.midY - origin.y)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Under Reduce Motion nothing travels: the docked thumbnail fades in at the
/// navigation bar's centre while the page's card fades out.
struct PurseDockedThumbnail: View {
    let dock: CGPoint
    let opacity: Double

    var body: some View {
        GeometryReader { proxy in
            let origin = proxy.frame(in: .global).origin
            CashCardFace()
                .frame(width: PurseMotion.Collapse.dockedWidth)
                .position(x: dock.x - origin.x, y: dock.y - origin.y)
                .opacity(opacity)
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Reports the frame, in the window, of the navigation bar above the view it
/// sits in, so SwiftUI can dock something at its centre.
struct NavigationBarFrameReader: UIViewRepresentable {
    let onChange: (CGRect) -> Void

    func makeUIView(context _: Context) -> ProbeView {
        ProbeView(onChange: onChange)
    }

    func updateUIView(_ view: ProbeView, context _: Context) {
        view.onChange = onChange
        view.report()
    }

    final class ProbeView: UIView {
        var onChange: (CGRect) -> Void
        private var reported: CGRect?

        init(onChange: @escaping (CGRect) -> Void) {
            self.onChange = onChange
            super.init(frame: .zero)
            isUserInteractionEnabled = false
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            report()
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            report()
        }

        func report() {
            guard let bar = navigationBar, let window else { return }

            let frame = bar.convert(bar.bounds, to: window)
            guard frame != reported else { return }

            reported = frame
            // Out of the layout pass that found it, so the update is not made
            // while SwiftUI is drawing.
            DispatchQueue.main.async { [onChange] in onChange(frame) }
        }

        private var navigationBar: UINavigationBar? {
            var responder: UIResponder? = self
            while let next = responder?.next {
                if let controller = next as? UIViewController, let navigation = controller.navigationController {
                    return navigation.navigationBar
                }
                responder = next
            }
            return nil
        }
    }
}

/// The wallet scroll view's offset from its resting top. Scroll geometry is an
/// iOS 18 API; on iOS 17 the value stays nil and the offset is read back from
/// where the card is.
struct ScrollOffsetReader: ViewModifier {
    @Binding var offset: CGFloat?

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content
                .onScrollGeometryChange(for: CGFloat.self) { geometry in
                    geometry.contentOffset.y + geometry.contentInsets.top
                } action: { _, newValue in
                    offset = newValue
                }
        } else {
            content
        }
    }
}
