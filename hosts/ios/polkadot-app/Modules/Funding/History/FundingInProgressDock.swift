import DesignSystem
import PolkadotUI
import SwiftUI

/// What is in flight, on one card floating above the tab bar. With more than
/// one session the card shows how many on the front one's icon, and a tap
/// opens them all in a panel over a scrim.
struct FundingInProgressDock: View {
    let cash: FundingCash
    var center: FundingActivityCenter = .shared

    @State private var isExpanded = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Room the history leaves at its end so the dock does not cover it.
    static let reservedHeight: CGFloat = rowHeight + 32
    static let rowHeight: CGFloat = 84
    private static let headerHeight: CGFloat = 48
    private static let maxListHeight: CGFloat = 560
    private static let cornerRadius: CGFloat = 24

    /// The front of the dock first: a delayed session, else the newest.
    static func ordered(_ items: [FundingActivityItem]) -> [FundingActivityItem] {
        items.sorted { lhs, rhs in
            if lhs.isDelayed != rhs.isDelayed { return lhs.isDelayed }
            return lhs.date > rhs.date
        }
    }

    var body: some View {
        let items = Self.ordered(center.inFlight)
        ZStack(alignment: .bottom) {
            if isExpanded {
                Color.black.opacity(0.48)
                    .ignoresSafeArea()
                    .onTapGesture { setExpanded(false) }
                    .transition(.opacity)
            }
            if let front = items.first {
                card(front: front, items: items)
                    .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: Self.cornerRadius))
                    .shadow(color: .black.opacity(0.45), radius: 16, y: 12)
                    .padding(.horizontal, DSSpacings.medium)
                    .padding(.bottom, DSSpacings.small)
                    .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
            }
        }
        // A session that finishes leaves the dock, and the dock slides away
        // with the last one.
        .animation(.spring(duration: PurseMotion.Dock.spring, bounce: 0.1), value: items.map(\.id))
        .onChange(of: items.count) { _, count in
            if count < 2 { isExpanded = false }
        }
    }
}

private extension FundingInProgressDock {
    @ViewBuilder
    func card(front: FundingActivityItem, items: [FundingActivityItem]) -> some View {
        if isExpanded, items.count > 1 {
            panel(items)
        } else {
            row(front, stackCount: items.count > 1 ? items.count : nil, showsDivider: false)
                .onTapGesture {
                    if items.count > 1 {
                        setExpanded(true)
                    } else {
                        center.open(intent: front.id)
                    }
                }
        }
    }

    func panel(_ items: [FundingActivityItem]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                setExpanded(false)
            } label: {
                HStack {
                    Text(.Funding.activityInProgress)
                        .typography(.bodyLargeEmphasized)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 13, weight: .semibold))
                }
                .foregroundStyle(.fgPrimary)
                .padding(.horizontal, DSSpacings.medium)
                .frame(height: Self.headerHeight)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            ScrollView {
                VStack(spacing: 0) {
                    ForEach(items) { item in
                        row(item, stackCount: nil, showsDivider: item.id != items.last?.id)
                            .onTapGesture { center.open(intent: item.id) }
                    }
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(height: min(CGFloat(items.count) * Self.rowHeight, Self.maxListHeight))
        }
    }

    func row(_ item: FundingActivityItem, stackCount: Int?, showsDivider: Bool) -> some View {
        FundingActivityRow(item: item, cash: cash, showsDivider: showsDivider, stackCount: stackCount)
            .frame(height: Self.rowHeight)
            .padding(.horizontal, DSSpacings.medium)
            .contentShape(Rectangle())
    }

    func setExpanded(_ expanded: Bool) {
        let animation: Animation = reduceMotion
            ? .easeInOut(duration: PurseMotion.Dock.spring)
            : .spring(duration: PurseMotion.Dock.spring, bounce: 0.1)
        withAnimation(animation) { isExpanded = expanded }
    }
}
