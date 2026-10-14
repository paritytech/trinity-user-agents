import SwiftUI

private enum AssetFundingStatusViewLayout {
    static let leadingSize = CGSize(width: 40, height: 40)
    static let headerSpacing: CGFloat = 16
    static let headerTextSpacing: CGFloat = 12
    static let detailsTopPadding: CGFloat = 8
    static let innerPadding: CGFloat = 16
    static let innerRadius: CGFloat = 24
    static let outerRadius: CGFloat = 32
    static let outerPaddingTop: CGFloat = 16
    static let outerPaddingHorizontal: CGFloat = 16
    static let outerPaddingBottom: CGFloat = 16
    static let collapsedPaddingVertical: CGFloat = 12
    static let collapsedPaddingHorizontal: CGFloat = 16
    static let actionButtonRadius: CGFloat = 12
    static let actionButtonPadding: CGFloat = 16
}

private struct FundingContentModel: Identifiable {
    let state: AssetFundingStatusView.FundingState
    let config: AssetFundingStatusView.StateConfig
    let details: AssetFundingStatusView.DetailsContent?

    var id: String { state.id }
}

private struct StateContentRowView: View {
    let model: FundingContentModel
    let title: String?
    let showsDisclosure: Bool
    let isExpanded: Bool

    var body: some View {
        HStack(alignment: isExpanded ? .top : .center, spacing: AssetFundingStatusViewLayout.headerSpacing) {
            leadingView

            VStack(alignment: .leading, spacing: AssetFundingStatusViewLayout.headerTextSpacing) {
                if let title {
                    Text(title)
                        .textStyle(.title16SemiBold())
                        .foregroundStyle(Color(.textAndIconsPrimaryDark))
                        .lineLimit(nil)
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text(model.config.subtitle)
                        .textStyle(.body14Regular())
                        .foregroundStyle(model.config.subtitleColor)

                    if isExpanded, let details = model.details {
                        detailsText(details)
                            .padding(.top, AssetFundingStatusViewLayout.detailsTopPadding)
                    }
                }
            }

            Spacer(minLength: 12)

            if showsDisclosure {
                Image(systemName: "chevron.up")
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .foregroundStyle(Color(.white100))
                    .frame(width: 24, height: 24)
            }
        }
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private var leadingView: some View {
        switch model.state.status {
        case let .inProgress(totalSeconds: totalSeconds, amountIn: _, amountOut: _):
            CountdownTimerView(
                totalTime: TimeInterval(totalSeconds),
                size: AssetFundingStatusViewLayout.leadingSize
            )
        default:
            if case let .icon(image) = model.config.leadingStyle {
                Image(image)
                    .frame(
                        width: AssetFundingStatusViewLayout.leadingSize.width,
                        height: AssetFundingStatusViewLayout.leadingSize.height
                    )
                    .background(Color(.white8), in: Circle())
            }
        }
    }

    @ViewBuilder
    private func detailsText(_ details: AssetFundingStatusView.DetailsContent) -> some View {
        switch details {
        case let .string(text):
            Text(text)
                .textStyle(.body16Regular())
                .foregroundStyle(Color(.textAndIconsTertiaryDark))
        case let .attributed(text):
            Text(text)
        }
    }
}

private struct ExpandedSheetContentView: View {
    let title: String
    let models: [FundingContentModel]
    let action: AssetFundingStatusView.ActionConfig?
    let onActionTap: (AssetFundingStatusView.ActionConfig) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: AssetFundingStatusViewLayout.headerTextSpacing) {
                    Text(title)
                        .textStyle(.title18SemiBold())
                        .foregroundStyle(Color(.textAndIconsPrimaryDark))
                        .lineLimit(nil)

                    ForEach(Array(models.enumerated()), id: \.element.id) { index, model in
                        if index > 0 {
                            Divider()
                                .background(Color(.white8))
                                .padding(.vertical, 8)
                        }
                        StateContentRowView(
                            model: model,
                            title: nil,
                            showsDisclosure: false,
                            isExpanded: true
                        )
                    }
                }
                .padding(AssetFundingStatusViewLayout.innerPadding)
                .background(
                    Color(.fill6),
                    in: RoundedRectangle(cornerRadius: AssetFundingStatusViewLayout.innerRadius)
                )
            }

            if let action {
                Button {
                    onActionTap(action)
                } label: {
                    Text(action.title)
                        .textStyle(.title16SemiBold())
                        .foregroundStyle(action.textColor)
                        .padding(AssetFundingStatusViewLayout.actionButtonPadding)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(
                    action.background,
                    in: RoundedRectangle(cornerRadius: AssetFundingStatusViewLayout.actionButtonRadius)
                )
                .frame(maxWidth: .infinity)
            }
        }
        .padding(.horizontal, AssetFundingStatusViewLayout.outerPaddingHorizontal)
        .padding(.top, AssetFundingStatusViewLayout.outerPaddingTop)
        .padding(.bottom, AssetFundingStatusViewLayout.outerPaddingBottom)
    }
}

public struct AssetFundingStatusView: View {
    // MARK: - Types

    public struct FundingState: Hashable, Identifiable {
        public enum Status: Hashable {
            case waiting
            case completed(amountIn: String, amountOut: String)
            case inProgress(totalSeconds: Int, amountIn: String, amountOut: String)
            case failed
        }

        public let id: String
        public let status: Status

        public init(id: String, status: Status) {
            self.id = id
            self.status = status
        }
    }

    public struct Configuration {
        public let titleExpanded: String
        public let titleCollapsed: String
        public let stateConfigs: StateConfigs

        public init(titleExpanded: String, titleCollapsed: String, stateConfigs: StateConfigs) {
            self.titleExpanded = titleExpanded
            self.titleCollapsed = titleCollapsed
            self.stateConfigs = stateConfigs
        }
    }

    public struct StateConfigs {
        public let waiting: StateConfig
        public let inProgress: StateConfig
        public let completed: StateConfig
        public let failed: StateConfig

        public init(
            waiting: StateConfig,
            inProgress: StateConfig,
            completed: StateConfig,
            failed: StateConfig
        ) {
            self.waiting = waiting
            self.inProgress = inProgress
            self.completed = completed
            self.failed = failed
        }

        public subscript(_ state: FundingState) -> StateConfig {
            switch state.status {
            case .waiting:
                waiting
            case .completed(amountIn: _, amountOut: _):
                completed
            case .inProgress(totalSeconds: _, amountIn: _, amountOut: _):
                inProgress
            case .failed:
                failed
            }
        }
    }

    public struct StateConfig {
        public let leadingStyle: LeadingStyle
        public let subtitle: String
        public let subtitleColor: Color
        public let details: DetailsContent?
        public let action: ActionConfig?

        public init(
            leadingStyle: LeadingStyle,
            subtitle: String,
            subtitleColor: Color,
            details: DetailsContent?,
            action: ActionConfig?
        ) {
            self.leadingStyle = leadingStyle
            self.subtitle = subtitle
            self.subtitleColor = subtitleColor
            self.details = details
            self.action = action
        }
    }

    public struct ActionConfig {
        public let title: String
        public let background: Color
        public let textColor: Color
        public let handler: (() -> Void)?

        public init(
            title: String,
            background: Color,
            textColor: Color,
            handler: (() -> Void)?
        ) {
            self.title = title
            self.background = background
            self.textColor = textColor
            self.handler = handler
        }
    }

    public enum DetailsContent {
        case string(String)
        case attributed(AttributedString)
    }

    public enum LeadingStyle {
        case icon(ImageResource)
        case progress
    }

    // MARK: - State

    @Binding public var states: [FundingState]
    @Binding public var isExpanded: Bool
    public let configuration: Configuration

    public init(
        states: Binding<[FundingState]>,
        isExpanded: Binding<Bool>,
        configuration: Configuration
    ) {
        _states = states
        _isExpanded = isExpanded
        self.configuration = configuration
    }

    // MARK: - View

    public var body: some View {
        collapsedView
            .hidden(isExpanded)
            .allowsHitTesting(!isExpanded)
            .sheet(isPresented: $isExpanded) {
                expandedSheetView
            }
    }

    // MARK: - Subviews

    private var expandedSheetView: some View {
        ExpandedSheetContentView(
            title: title,
            models: orderedContentModels,
            action: action,
            onActionTap: handleActionTap
        )
        .presentationDetents([.fraction(0.33), .medium, .large])
        .presentationDragIndicator(.visible)
    }

    @ViewBuilder
    private var collapsedView: some View {
        if let model = collapsedContentModel {
            StateContentRowView(
                model: model,
                title: title,
                showsDisclosure: true,
                isExpanded: false
            )
            .onTapGesture {
                isExpanded.toggle()
            }
            .padding(.vertical, AssetFundingStatusViewLayout.collapsedPaddingVertical)
            .padding(.horizontal, AssetFundingStatusViewLayout.collapsedPaddingHorizontal)
            .background(Color(.backgroundTertiary), in: Rectangle())
        }
    }

    // MARK: - Helpers

    private static func makeAmountConversionText(incoming: String, outgoing: String) -> AttributedString {
        var amountIn = AttributedString("\(incoming) ≈ ")
        amountIn.font = .body16Regular()
        amountIn.foregroundColor = Color(.textAndIconsTertiaryDark)

        var amountOut = AttributedString(outgoing)
        amountOut.font = .body16Regular()
        amountOut.foregroundColor = Color(.textAndIconsPrimaryDark)

        return amountIn + amountOut
    }

    private func config(for state: FundingState) -> StateConfig {
        configuration.stateConfigs[state]
    }

    private var title: String {
        isExpanded ? configuration.titleExpanded : configuration.titleCollapsed
    }

    private var orderedStates: [FundingState] {
        states.sorted { lhs, rhs in
            let leftKey = sortKey(for: lhs.status)
            let rightKey = sortKey(for: rhs.status)
            if leftKey != rightKey {
                return leftKey < rightKey
            }
            return lhs.id < rhs.id
        }
    }

    private var orderedContentModels: [FundingContentModel] {
        orderedStates.map { state in
            FundingContentModel(
                state: state,
                config: config(for: state),
                details: details(for: state)
            )
        }
    }

    private var collapsedContentModel: FundingContentModel? {
        orderedContentModels.first
    }

    private func handleActionTap(_ action: ActionConfig) {
        isExpanded = false
        action.handler?()
    }

    private var action: ActionConfig? {
        if hasFailed, let action = configuration.stateConfigs.failed.action {
            return action
        }

        if hasCompleted, let action = configuration.stateConfigs.completed.action {
            return action
        }

        if hasActive, let action = hideAction {
            return action
        }

        return nil
    }

    private var hideAction: ActionConfig? {
        configuration.stateConfigs.waiting.action ?? configuration.stateConfigs.inProgress.action
    }

    private var hasFailed: Bool {
        states.contains { state in
            if case .failed = state.status {
                return true
            }
            return false
        }
    }

    private var hasActive: Bool {
        states.contains { state in
            switch state.status {
            case .waiting,
                 .inProgress(totalSeconds: _, amountIn: _, amountOut: _):
                true
            default:
                false
            }
        }
    }

    private var hasCompleted: Bool {
        states.contains { state in
            if case .completed(amountIn: _, amountOut: _) = state.status {
                return true
            }
            return false
        }
    }

    private func sortKey(for status: FundingState.Status) -> Int {
        switch status {
        case .waiting:
            0
        case .inProgress(totalSeconds: _, amountIn: _, amountOut: _):
            1
        case .failed:
            2
        case .completed(amountIn: _, amountOut: _):
            3
        }
    }

    private func details(for state: FundingState) -> DetailsContent? {
        switch state.status {
        case let .inProgress(totalSeconds: _, amountIn: amountIn, amountOut: amountOut),
             let .completed(amountIn: amountIn, amountOut: amountOut):
            .attributed(
                Self.makeAmountConversionText(incoming: amountIn, outgoing: amountOut)
            )
        default:
            config(for: state).details
        }
    }
}

// MARK: - Configuration Defaults

public extension AssetFundingStatusView.Configuration {
    static func fundingDigitalDollarConfiguration(
        onCompletedAction: (() -> Void)? = nil,
        onFailedAction: (() -> Void)? = nil
    ) -> AssetFundingStatusView.Configuration {
        let actionHide = AssetFundingStatusView.ActionConfig(
            title: String(localized: .Common.hide),
            background: Color(.fill6),
            textColor: Color(.textAndIconsPrimaryDark),
            handler: nil
        )

        return AssetFundingStatusView.Configuration(
            titleExpanded: String(localized: .AssetFunding.fundingDigitalDollarTitleExpanded),
            titleCollapsed: String(localized: .AssetFunding.fundingDigitalDollarTitleCollapsed),
            stateConfigs: AssetFundingStatusView.StateConfigs(
                waiting: AssetFundingStatusView.StateConfig(
                    leadingStyle: .icon(.waitingIcon),
                    subtitle: String(localized: .AssetFunding.fundingDigitalDollarWaitingSubtitle),
                    subtitleColor: Color(.yellowBadge),
                    details: .string(
                        String(localized: .AssetFunding.fundingDigitalDollarWaitingDetails)
                    ),
                    action: actionHide
                ),
                inProgress: AssetFundingStatusView.StateConfig(
                    leadingStyle: .progress,
                    subtitle: String(localized: .Common.inProgress),
                    subtitleColor: Color(.systemSuccess),
                    details: nil,
                    action: actionHide
                ),
                completed: AssetFundingStatusView.StateConfig(
                    leadingStyle: .icon(.completedIcon),
                    subtitle: String(localized: .Common.completed),
                    subtitleColor: Color(.brandGreen),
                    details: nil,
                    action: AssetFundingStatusView.ActionConfig(
                        title: String(localized: .Common.done),
                        background: Color(.white100),
                        textColor: Color(.textAndIconsPrimaryLight),
                        handler: onCompletedAction
                    )
                ),
                failed: AssetFundingStatusView.StateConfig(
                    leadingStyle: .icon(.failedIcon),
                    subtitle: String(localized: .Common.transactionFailed),
                    subtitleColor: Color(.systemError),
                    details: .string(String(localized: .AssetFunding.fundingDigitalDollarFailedDetails)),
                    action: AssetFundingStatusView.ActionConfig(
                        title: String(localized: .Common.emailSupport),
                        background: Color(.white100),
                        textColor: Color(.textAndIconsPrimaryLight),
                        handler: onFailedAction
                    )
                )
            )
        )
    }
}

private extension View {
    @ViewBuilder
    func hidden(_ isHidden: Bool) -> some View {
        if isHidden {
            hidden()
        } else {
            self
        }
    }
}
