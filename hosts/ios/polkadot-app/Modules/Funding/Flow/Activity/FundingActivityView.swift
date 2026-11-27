import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Top-ups and withdrawals on one screen: those in flight under In progress,
/// the ended ones under Completed, newest first. A row opens its session.
struct FundingHistoryView: View {
    let model: FundingActivityViewModel
    let presenter: FundingActivityPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: String(localized: .Funding.historyTitle), onBack: presenter.back)

            if model.state.inFlight.isEmpty, model.state.history.isEmpty {
                Text(.Funding.historyEmpty)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .padding(.top, DSSpacings.large)
                Spacer(minLength: 0)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: DSSpacings.large) {
                        section(String(localized: .Common.inProgress), items: model.state.inFlight)
                        section(String(localized: .Common.completed), items: model.state.history)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func section(_ title: String, items: [FundingActivityItem]) -> some View {
        if !items.isEmpty {
            VStack(alignment: .leading, spacing: DSSpacings.small) {
                Text(verbatim: title)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                ForEach(items) { item in
                    if let content = model.state.rows[item.id] {
                        FundingActivityRow(content: content, arrives: model.state.arriving.contains(item.id))
                            .contentShape(Rectangle())
                            .onTapGesture { presenter.open(item) }
                    }
                }
            }
        }
    }
}

/// What is in flight in one direction, shown when Get or Send money out is
/// tapped while a session is still running: the sessions, a way to the
/// history, and a button to start another.
struct FundingPendingView: View {
    let direction: FundingDirection
    let model: FundingActivityViewModel
    let presenter: FundingActivityPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            HStack {
                Text(verbatim: title)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                Spacer()
                Button(action: presenter.showHistory) {
                    Image(systemName: "clock.arrow.circlepath")
                        .font(.system(size: 16, weight: .semibold))
                }
                .buttonStyle(.dsIcon(style: .secondary, shape: .pill, size: .small))
                .accessibilityLabel(Text(.Funding.historyTitle))
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(items) { item in
                        if let content = model.state.rows[item.id] {
                            FundingActivityRow(content: content)
                                .contentShape(Rectangle())
                                .onTapGesture { presenter.open(item) }
                        }
                    }
                }
            }

            DSButton(buttonTitle, expands: true, action: presenter.startNew)
        }
    }

    private var items: [FundingActivityItem] {
        model.state.inFlight
    }

    private var title: String {
        direction == .in ? String(localized: .Funding.activityToppingUp) : String(localized: .transferStatusSending)
    }

    private var buttonTitle: String {
        direction == .in ? String(localized: .Funding.pendingNewTopUp) : String(localized: .Funding.pendingSendOut)
    }
}

/// One session in a list: its amount, what it is doing or when it ended, and
/// the rail it went by.
struct FundingActivityRow: View {
    let content: FundingActivityRowContent
    /// Just finished: the row arrives and its icon plays the completion.
    var arrives = false

    private static let iconSize: CGFloat = 20

    var body: some View {
        HStack(alignment: .top, spacing: DSSpacings.small) {
            FundingActivityIcon(icon: content.icon, animatesCompletion: arrives)
                .frame(width: Self.iconSize, height: Self.iconSize)
            VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
                Text(verbatim: content.title)
                    .typography(.bodyMediumEmphasized)
                    .foregroundStyle(content.titleTone.color)
                    .lineLimit(1)
                Text(content.subtitle)
                    .typography(.bodySmall)
                    .foregroundStyle(content.isSubtitleWarning ? Color.fgWarning : Color.fgSecondary)
                    .lineLimit(1)
            }
            Spacer(minLength: DSSpacings.small)
            if let pill = content.pill {
                Text(verbatim: pill)
                    .typography(.labelSmall)
                    .foregroundStyle(.fgSecondary)
                    .padding(.horizontal, DSSpacings.small)
                    .padding(.vertical, DSSpacings.extraTiny)
                    .background(.bgSurfaceNested, in: Capsule())
            }
        }
        .padding(DSSpacings.medium)
        .background(.bgSurfaceMain, in: RoundedRectangle(cornerRadius: DSRadii.large))
        .modifier(FundingArrival(isArriving: arrives))
    }
}

/// A row's icon: a ring while in flight (amber when slow), an arrow once value
/// moved, a cross once it failed.
struct FundingActivityIcon: View {
    let icon: FundingActivityRowContent.Icon
    /// Plays the ring turning into what happened, for a row that has just
    /// arrived.
    var animatesCompletion = false

    var body: some View {
        switch icon {
        case let .pending(delayed):
            ProgressView()
                .controlSize(.mini)
                .tint(delayed ? Color.fgWarning : Color.fgSecondary)
        case .received:
            FundingCompletionGlyph(kind: .received, animated: animatesCompletion)
                .foregroundStyle(.fgSuccess)
        case .sent:
            FundingCompletionGlyph(kind: .sent, animated: animatesCompletion)
                .foregroundStyle(.fgSecondary)
        case .failed:
            FundingCompletionGlyph(kind: .failed, animated: animatesCompletion)
                .foregroundStyle(.fgError)
        }
    }
}

private extension FundingActivityRowContent.Tone {
    var color: Color {
        switch self {
        case .success: .fgSuccess
        case .primary: .fgPrimary
        case .secondary: .fgSecondary
        }
    }
}
