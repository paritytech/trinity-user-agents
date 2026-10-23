import SwiftUI
import ExternalAccessibility
import PolkadotUI
import DesignSystem

struct AssetDetailsView: View {
    @State var viewModel: AssetDetailsViewModelProtocol
    var isExpanded: Bool = false
    var onCardTapped: () -> Void
    var overscroll: CGFloat = 0
    var onCollapse: (() -> Void)?
    /// How visible the page's own card is: the wallet draws the card itself
    /// while it shrinks into the header, so this one stands aside.
    var cardOpacity: Double = 1
    /// The funding lists the open Purse draws.
    var fundingCenter: FundingActivityCenter
    var cash: FundingCash

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    #if DEBUG
        /// Which holdings the coins draw, for reviewing the depiction. Debug builds only.
        @State private var testDataMode = CoinageTestDataMode.live
        /// Plain or varied coins, while the two are being compared. Debug builds only.
        @State private var coinSet = CoinageScene.CoinSet.shipped
    #endif

    init(
        viewModel: AssetDetailsViewModelProtocol = AssetDetailsViewModel(),
        isExpanded: Bool = false,
        onCardTapped: @escaping () -> Void,
        overscroll: CGFloat = 0,
        onCollapse: (() -> Void)? = nil,
        cardOpacity: Double = 1,
        fundingCenter: FundingActivityCenter,
        cash: FundingCash
    ) {
        _viewModel = State(initialValue: viewModel)
        self.isExpanded = isExpanded
        self.onCardTapped = onCardTapped
        self.overscroll = overscroll
        self.onCollapse = onCollapse
        self.cardOpacity = cardOpacity
        self.fundingCenter = fundingCenter
        self.cash = cash
    }

    var body: some View {
        DSExpandableCardLayout(
            isExpanded: isExpanded,
            overscroll: overscroll,
            onCollapse: onCollapse,
            card: { headerCard },
            details: { expandedBody }
        )
    }

    @ViewBuilder
    private var headerCard: some View {
        if let balanceCardModel = viewModel.balanceCardModel {
            balanceCard(balanceCardModel)
                .onTapGesture { onCardTapped() }
                .background { cardMark }
                .opacity(isExpanded ? cardOpacity : 1)
                .animation(balanceAnimation, value: viewModel.coinageBreakdown?.amounts.total)
        }
    }

    /// Where the card is while the Purse scrolls, for the card the wallet draws
    /// shrinking into the header and the sticky bar under it.
    @ViewBuilder
    private var cardMark: some View {
        if isExpanded {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: PurseScroll.CardKey.self,
                    value: PurseScroll.Card(
                        top: proxy.frame(in: .named(PurseScroll.space)).minY,
                        bottom: proxy.frame(in: .named(PurseScroll.space)).maxY,
                        balance: viewModel.coinageBreakdown?.amounts.total,
                        frame: proxy.frame(in: .global)
                    )
                )
            }
        }
    }

    @ViewBuilder
    private var expandedBody: some View {
        VStack(spacing: 16) {
            if viewModel.showsAccountBackupPending {
                AccountBackupPendingView()
            }
            // The Total panel tucks under the tiles.
            VStack(spacing: -Self.panelTuck) {
                Group {
                    if viewModel.showsBackupNotification {
                        backupCard()
                    } else {
                        actions()
                    }
                }
                .zIndex(1)
                #if DEBUG
                    // The panel stays on a wallet that holds no coinage, which is where the test
                    // sets are most useful.
                    if viewModel.balanceCardModel != nil {
                        PurseTotalPanel(
                            breakdown: testDataMode.applied(to: viewModel.coinageBreakdown ?? .testDataPlaceholder),
                            topInset: Self.panelTuck,
                            coinSet: coinSet
                        )
                    }
                #else
                    if let breakdown = viewModel.coinageBreakdown,
                       viewModel.balanceCardModel != nil {
                        PurseTotalPanel(breakdown: breakdown, topInset: Self.panelTuck)
                    }
                #endif
            }
            FundingActivityView(
                cash: cash,
                currentBalance: viewModel.coinageBreakdown?.amounts.total,
                center: fundingCenter
            )

            #if DEBUG || TESTNET_FEATURE
                HStack {
                    VStack { Divider().background(Color.fgPrimary) }
                    Text(verbatim: "Debug features")
                        .typography(.labelMedium)
                        .foregroundStyle(Color.fgPrimary)
                    VStack { Divider().background(Color.fgPrimary) }
                }
            #endif

            #if DEBUG
                CoinageTestDataSwitch(mode: $testDataMode)

                CoinageCoinSetSwitch(set: $coinSet)
            #endif

            #if TESTNET_FEATURE
                testnetTopUpButton()
            #endif
        }
    }

    private static let panelTuck: CGFloat = 8

    /// The balance counting to its new figure.
    private var balanceAnimation: Animation? {
        reduceMotion ? .easeInOut(duration: PurseMotion.Balance.countUp) :
            .easeOut(duration: PurseMotion.Balance.countUp)
    }

    private func balanceCard(
        _ balanceCardModel: AssetDetailsBalanceCard.ViewModel
    ) -> some View {
        AssetDetailsBalanceCard(
            viewModel: balanceCardModel,
            isUpdating: viewModel.isUpdating,
            isExpanded: isExpanded
        )
    }

    private func backupCard() -> some View {
        WalletBackupNotificationCard(
            isUpdating: viewModel.isUpdating,
            onSync: viewModel.onBackupSync,
            onCancel: viewModel.onBackupCancel,
            onWhyUpdate: viewModel.onBackupWhyUpdate
        )
    }

    private func actions() -> some View {
        HStack(spacing: DSSpacings.small) {
            actionCard(
                title: String(localized: .Funding.cardGet),
                subtitle: String(localized: .Funding.cardGetSubtitle(symbol: cash.symbol)),
                arrowRotation: .zero,
                isLoading: viewModel.isTopUpInProgress
            ) {
                viewModel.onTopUp?()
            }
            .accessibilityId(AccessibilityID.Wallet.addFundsButton)

            actionCard(
                title: String(localized: .Funding.cardSend),
                subtitle: String(localized: .Funding.cardSendSubtitle(symbol: cash.symbol)),
                arrowRotation: .degrees(180),
                isLoading: false
            ) {
                viewModel.onSendMoney?()
            }
            .accessibilityId(AccessibilityID.Wallet.sendPaymentButton)
        }
    }

    private func actionCard(
        title: String,
        subtitle: String,
        arrowRotation: Angle,
        isLoading: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            VStack(spacing: DSSpacings.small) {
                HStack(spacing: DSSpacings.small) {
                    if isLoading {
                        ProgressView()
                            .progressViewStyle(.circular)
                            .tint(.fgPrimaryInverted)
                    } else {
                        Image(.iconArrowUp16)
                            .renderingMode(.template)
                            .resizable()
                            .frame(width: 18, height: 18)
                            .rotationEffect(arrowRotation)
                    }
                    Text(title)
                        .typography(.titleLarge)
                }
                .foregroundStyle(Color.fgPrimaryInverted)

                Text(subtitle)
                    .typography(.bodySmall)
                    .foregroundStyle(Color.fgSecondaryInverted)
                    .multilineTextAlignment(.center)
            }
            .padding(.horizontal, DSSpacings.medium)
            .padding(.vertical, DSSpacings.smallIncreased)
            .frame(maxWidth: .infinity)
            .frame(height: 96)
            .background(Color.bgActionPrimary, in: RoundedRectangle(cornerRadius: DSRadii.mediumIncreased))
        }
        .buttonStyle(.plain)
        .disabled(isLoading)
    }

    #if TESTNET_FEATURE
        private func testnetTopUpButton() -> some View {
            Button {
                viewModel.onTestnetTopUp?()
            } label: {
                Group {
                    if viewModel.isTestnetTopUpInProgress {
                        ProgressView()
                            .progressViewStyle(.circular)
                            .tint(.fgPrimaryInverted)
                    } else {
                        Text(verbatim: "Faucet Top Up")
                            .textStyle(.body14SemiBold())
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
                .foregroundStyle(Color.fgPrimaryInverted)
                .background(.bgActionPrimary, in: RoundedRectangle(cornerRadius: 12))
            }
            .disabled(viewModel.isTestnetTopUpInProgress)
        }
    #endif
}

/// Total, the coins it is made of, and how much of it is still clearing.
private struct PurseTotalPanel: View {
    let breakdown: CoinageBalanceBreakdownViewModel
    /// How far the panel sits under the tiles above it.
    var topInset: CGFloat = 0
    /// Plain or varied coins, while the two are being compared.
    var coinSet: CoinageScene.CoinSet = .shipped

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// How the coins came out, reported by the view as it lays them out.
    @State private var coinMetrics = CoinageCoinsView.Metrics()

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            VStack(spacing: 0) {
                Text(.coinageSummaryTitle)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityId(AccessibilityID.Wallet.coinageHeader)

                totalHeadline
            }

            // Nothing held, nothing to draw: no strip to reserve height for and no split to show.
            if !breakdown.strip.isEmpty {
                CoinageCoinsView(coins: breakdown.strip, isExpanded: false, coinSet: coinSet, metrics: $coinMetrics)
                    .frame(height: CoinageStripLayout.Options().height, alignment: .top)
                    .clipped()

                CoinageSplitBar(clearing: breakdown.amounts.gainingPrivacy, total: breakdown.amounts.total)

                splitLabels
            }
        }
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .padding(.top, DSSpacings.mediumIncreased + topInset)
        .padding(.bottom, DSSpacings.mediumIncreased)
        .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
    }

    private var totalHeadline: some View {
        DSAmount(
            amount: breakdown.totalBalance,
            symbol: breakdown.symbol,
            typography: .displaySmall
        )
        .lineLimit(1)
        .contentTransition(reduceMotion ? .opacity : .numericText(value: totalValue))
        .animation(.easeOut(duration: PurseMotion.Balance.countUp), value: totalValue)
        .accessibilityId(AccessibilityID.Wallet.coinageTotalBalanceValue)
        .foregroundStyle(Color.fgPrimary)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var totalValue: Double {
        NSDecimalNumber(decimal: breakdown.amounts.total).doubleValue
    }

    /// "Clearing $X" while anything is, and "Ready $Y": on either side when both
    /// show, centred when only Ready does.
    private var splitLabels: some View {
        HStack {
            if breakdown.amounts.gainingPrivacy > 0 {
                splitLabel(.coinageLoading, value: breakdown.gainingPrivacyBalance)
                Spacer(minLength: DSSpacings.small)
                splitLabel(.coinageSpendable, value: breakdown.availableNowBalance)
            } else {
                splitLabel(.coinageSpendable, value: breakdown.availableNowBalance)
                    .frame(maxWidth: .infinity)
            }
        }
    }

    private func splitLabel(_ key: LocalizedStringResource, value: String) -> some View {
        HStack(spacing: DSSpacings.extraSmall) {
            Text(key)
                .foregroundStyle(.fgSecondary)
            Text(verbatim: value)
                .foregroundStyle(.fgPrimary)
                .monospacedDigit()
        }
        .typography(.bodyMedium)
    }
}
