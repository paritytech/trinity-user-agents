import DesignSystem
import SwiftUI
import TrUAPIHost

/// A session's timeline: its three milestones, labelled, with two dots
/// between each that fill as the session moves. Done milestones are a green
/// check joined in green, the dot the session is at is white (amber when it is
/// slow), upcoming ones grey, and a failed milestone red.
struct FundingStepBar: View {
    enum State: Equatable {
        case done
        case current(isAmber: Bool)
        case upcoming
        case failed
    }

    struct Step: Equatable {
        let title: String
        let state: State
    }

    let steps: [Step]
    /// How far the session is past the last done milestone: 0 at it, then 1
    /// or 2 for the dots towards the next.
    var between = 1

    private static let size: CGFloat = 22
    private static let dot: CGFloat = 8

    /// Which dot towards the next milestone a band stands at.
    static func between(_ band: FundingBand?) -> Int {
        switch band {
        case .checkingPayment,
             .adding,
             .sending: 2
        default: 1
        }
    }

    /// A milestone's name; a withdrawal's second one is the provider's.
    static func title(_ step: FundingStep, direction: FundingDirection) -> String {
        switch step {
        case .started: String(localized: .Funding.progressStepStarted)
        case .received:
            String(localized: direction == .out ? .Funding.progressStepReceivedOut : .Funding.progressStepReceived)
        case .added: String(localized: .Funding.progressStepAdded)
        case .sent: String(localized: .Funding.progressStepSent)
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                if index > 0 {
                    segment(into: step.state)
                }
                mark(step.state)
                    .overlay(alignment: .top) {
                        Text(verbatim: step.title)
                            .typography(.labelSmallEmphasized)
                            .foregroundStyle(labelColor(step.state))
                            .lineLimit(1)
                            .fixedSize()
                            .offset(y: Self.size + DSSpacings.extraSmall)
                    }
            }
        }
        .padding(.bottom, 20)
        .padding(.horizontal, DSSpacings.medium)
        .animation(.easeInOut, value: steps)
        .animation(.easeInOut, value: between)
    }
}

private extension FundingStepBar {
    enum DotState {
        case done
        case current(isAmber: Bool)
        case upcoming
    }

    @ViewBuilder
    func mark(_ state: State) -> some View {
        switch state {
        case .done:
            symbol("checkmark", color: .white)
                .background(Color.fgSuccess, in: Circle())
        case .current,
             .upcoming:
            symbol("checkmark", color: .fgTertiary)
                .background(Color.bgSurfaceNested, in: Circle())
        case .failed:
            symbol("xmark", color: .white)
                .background(Color.fgError, in: Circle())
        }
    }

    func symbol(_ name: String, color: Color) -> some View {
        Image(systemName: name)
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(color)
            .frame(width: Self.size, height: Self.size)
    }

    /// The line and two dots leading into a milestone in `state`.
    func segment(into state: State) -> some View {
        let dots = dots(into: state)
        return HStack(spacing: 0) {
            line(filled: dots[0].isFilled)
            dotMark(dots[0])
            line(filled: dots[1].isFilled)
            dotMark(dots[1])
            line(filled: state == .done)
        }
    }

    /// The two dots before a milestone: both done once it is reached, the
    /// session's position on the way to the current one, grey after it.
    func dots(into state: State) -> [DotState] {
        switch state {
        case .done:
            [.done, .done]
        case let .current(isAmber):
            (0 ..< 2).map { index in
                if index < between - 1 { return .done }
                return index == max(between - 1, 0) ? .current(isAmber: isAmber) : .upcoming
            }
        case .upcoming,
             .failed:
            [.upcoming, .upcoming]
        }
    }

    func dotMark(_ state: DotState) -> some View {
        Circle()
            .fill(dotColor(state))
            .frame(width: Self.dot, height: Self.dot)
    }

    func dotColor(_ state: DotState) -> Color {
        switch state {
        case .done: .fgSuccess
        case let .current(isAmber): isAmber ? .fgWarning : .fgPrimary
        case .upcoming: .bgSurfaceNested
        }
    }

    func line(filled: Bool) -> some View {
        Rectangle()
            .fill(filled ? Color.fgSuccess : Color.bgSurfaceNested)
            .frame(height: 2)
            .frame(maxWidth: .infinity)
    }

    func labelColor(_ state: State) -> Color {
        switch state {
        case .done: .fgSuccess
        case .current,
             .upcoming: .fgTertiary
        case .failed: .fgError
        }
    }
}

private extension FundingStepBar.DotState {
    var isFilled: Bool {
        if case .done = self { return true }
        return false
    }
}
