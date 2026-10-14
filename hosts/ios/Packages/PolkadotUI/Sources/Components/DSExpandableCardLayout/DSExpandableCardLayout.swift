import SwiftUI
import UIKit

/// Header with details underneath while expanded. The host scrolls it. `overscroll` is the host's
/// rubber-band distance past the top, which the header cancels out so a pull stretches only the
/// details. Dragging the header itself down past a threshold collapses. iOS 17 has no drag: the
/// scroll view owns every touch there, so the host's close button is the only way back.
public struct DSExpandableCardLayout<Card: View, Details: View>: View {
    private let isExpanded: Bool
    private let overscroll: CGFloat
    private let onCollapse: (() -> Void)?
    private let card: () -> Card
    private let details: () -> Details

    @State private var dragOffset: CGFloat = 0
    @State private var containerHeight: CGFloat = 0
    @GestureState private var isDragging = false

    private let maxBlur: CGFloat = 12

    public init(
        isExpanded: Bool,
        overscroll: CGFloat = 0,
        onCollapse: (() -> Void)? = nil,
        @ViewBuilder card: @escaping () -> Card,
        @ViewBuilder details: @escaping () -> Details
    ) {
        self.isExpanded = isExpanded
        self.overscroll = overscroll
        self.onCollapse = onCollapse
        self.card = card
        self.details = details
    }

    public var body: some View {
        VStack(spacing: 16) {
            header
                .zIndex(1)

            if isExpanded {
                details()
                    .offset(y: dragOffset)
                    .blur(radius: dragProgress * maxBlur)
                    .opacity(1 - dragProgress)
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .onAppear { containerHeight = Self.screenHeight }
        .onChange(of: isExpanded) { dragOffset = 0 }
        .onChange(of: isDragging) { _, dragging in
            guard !dragging else { return }
            finishDrag()
        }
    }
}

private extension DSExpandableCardLayout {
    /// The gesture goes on top of the offset, so its coordinate space stays put while the header
    /// moves; measured inside the offset, the translation oscillates with the header's own motion.
    /// iOS 18+ lets a high-priority drag claim touches that begin on the header, so the scroll view
    /// never cancels it. iOS 17 cancels any child drag once its own pan recognises, so the header
    /// carries no gesture there.
    @ViewBuilder
    var header: some View {
        let offsetCard = card().offset(y: isExpanded ? dragOffset - overscroll : 0)

        if #available(iOS 18, *) {
            offsetCard.highPriorityGesture(collapseGesture, including: isExpanded ? .all : .subviews)
        } else {
            offsetCard
        }
    }

    var dragRange: CGFloat {
        if #available(iOS 26, *) {
            max(containerHeight / 4, 1)
        } else {
            max(containerHeight / 6, 1)
        }
    }

    var dragProgress: CGFloat {
        min(dragOffset / dragRange, 1)
    }

    var collapseGesture: some Gesture {
        DragGesture(minimumDistance: 10)
            .updating($isDragging) { _, state, _ in state = true }
            .onChanged { value in
                guard isExpanded else { return }
                dragOffset = max(0, value.translation.height)
            }
    }

    /// Runs when the gesture state resets, which happens on release and on cancellation alike.
    func finishDrag() {
        guard isExpanded, dragOffset > 0 else { return }
        let shouldCollapse = dragOffset > dragRange
        withAnimation(.spring(duration: 0.45, bounce: 0.15)) {
            dragOffset = 0
        }
        if shouldCollapse { onCollapse?() }
    }

    static var screenHeight: CGFloat {
        UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.screen.bounds.height }
            .first ?? 0
    }
}
