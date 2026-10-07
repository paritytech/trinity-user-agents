import SwiftUI
import PolkadotUI

struct WalletView: View {
    @State var viewModel: WalletViewModelProtocol = WalletViewModel()
    @State private var viewHeight: CGFloat = 0
    @State private var scrollAtTop: Bool = true
    @Namespace private var cardNamespace
    private let peekHeight: CGFloat = 64
    private let scrollTopAnchor = "walletScrollTop"
    @State private var overscroll: CGFloat = 0

    var body: some View {
        ZStack(alignment: .top) {
            ScrollViewReader { scrollProxy in
                ScrollView {
                    ZStack(alignment: .top) {
                        assetCard
                        identityCard
                    }
                    .padding(.horizontal, 24)
                    .padding(.top, 16)
                    .onGeometryChange(for: CGFloat.self) { proxy in
                        proxy.size.height + proxy.safeAreaInsets.bottom
                    } action: {
                        viewHeight = $0
                    }
                    .id(scrollTopAnchor)
                }
                .safeAreaInset(edge: .bottom) {
                    if viewModel.expandedSection == .assetDetails {
                        AssetDetailsFundingBar(viewModel: viewModel.assetDetailsViewModel)
                    }
                }
                .modifier(OverscrollReader(overscroll: $overscroll))
                .onChange(of: viewModel.expandedSection) { _, newValue in
                    guard newValue == .none else { return }
                    withAnimation(.spring(duration: 0.45, bounce: 0.15)) {
                        scrollProxy.scrollTo(scrollTopAnchor, anchor: .top)
                    }
                }
            }
        }
        .animation(.spring(duration: 0.45, bounce: 0.15), value: viewModel.expandedSection)
    }

    @ViewBuilder
    private var identityCard: some View {
        IdentityDetailsViewLayout(
            viewModel: viewModel.identityDetailsViewModel,
            isExpanded: viewModel.expandedSection == .identityDetails,
            onCardTapped: { viewModel.onUsername?() },
            overscroll: overscroll,
            onCollapse: { viewModel.onCollapse?() }
        )
        .matchedGeometryEffect(id: "identity", in: cardNamespace)
        .scaleEffect(identityScale)
        .opacity(identityOpacity)
        .offset(y: identityOffsetY)
        .zIndex(identityZIndex)
        .allowsHitTesting(identityHitTest)
    }

    @ViewBuilder
    private var assetCard: some View {
        AssetDetailsView(
            viewModel: viewModel.assetDetailsViewModel,
            isExpanded: viewModel.expandedSection == .assetDetails,
            onCardTapped: {
                viewModel.onBalance?()
            },
            overscroll: overscroll,
            onCollapse: { viewModel.onCollapse?() }
        )
        .matchedGeometryEffect(id: "asset", in: cardNamespace)
        .scaleEffect(assetScale)
        .opacity(assetOpacity)
        .offset(y: assetOffsetY)
        .zIndex(assetZIndex)
        .allowsHitTesting(assetHitTest)
    }

    private var offScreenOffset: CGFloat {
        max(viewHeight, 1_000)
    }

    private var identityScale: CGFloat {
        1.0
    }

    private var identityOpacity: Double {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            1
        case .assetDetails:
            0
        }
    }

    private var identityOffsetY: CGFloat {
        switch viewModel.expandedSection {
        case .none:
            peekHeight
        case .identityDetails:
            0
        case .assetDetails:
            offScreenOffset
        }
    }

    private var identityZIndex: Double {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            1
        case .assetDetails:
            0
        }
    }

    private var identityHitTest: Bool {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            true
        case .assetDetails:
            false
        }
    }

    private var assetScale: CGFloat {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            1.0
        case .identityDetails:
            0.95
        }
    }

    private var assetOpacity: Double {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            1
        case .identityDetails:
            0
        }
    }

    private var assetOffsetY: CGFloat {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails,
             .identityDetails:
            0
        }
    }

    private var assetZIndex: Double {
        switch viewModel.expandedSection {
        case .none:
            0
        case .assetDetails:
            2
        case .identityDetails:
            0
        }
    }

    private var assetHitTest: Bool {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            true
        case .identityDetails:
            false
        }
    }
}

/// The host scroll view's rubber-band distance past the top. Scroll geometry is an iOS 18 API; on
/// iOS 17 the value stays 0, so an expanded header stretches with its details on a pull.
private struct OverscrollReader: ViewModifier {
    @Binding var overscroll: CGFloat

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content
                .onScrollGeometryChange(for: CGFloat.self) { geometry in
                    max(0, -(geometry.contentOffset.y + geometry.contentInsets.top))
                } action: { _, newValue in
                    overscroll = newValue
                }
        } else {
            content
        }
    }
}
