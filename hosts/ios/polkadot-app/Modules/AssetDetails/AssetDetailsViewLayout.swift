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

    init(
        viewModel: AssetDetailsViewModelProtocol = AssetDetailsViewModel(),
        isExpanded: Bool = false,
        onCardTapped: @escaping () -> Void,
        overscroll: CGFloat = 0,
        onCollapse: (() -> Void)? = nil
    ) {
        _viewModel = State(initialValue: viewModel)
        self.isExpanded = isExpanded
        self.onCardTapped = onCardTapped
        self.overscroll = overscroll
        self.onCollapse = onCollapse
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
        }
    }

    @ViewBuilder
    private var expandedBody: some View {
        VStack(spacing: 16) {
            if viewModel.showsAccountBackupPending {
                AccountBackupPendingView()
            }
            if viewModel.showsBackupNotification {
                backupCard()
            } else {
                actions()
            }
            if let breakdown = viewModel.coinageBreakdown,
               viewModel.balanceCardModel != nil {
                CoinageBalanceBreakdownView(breakdown: breakdown)
            }

            #if TESTNET_FEATURE
                HStack {
                    VStack { Divider().background(Color.fgPrimary) }
                    Text(verbatim: "Debug features")
                        .typography(.labelMedium)
                        .foregroundStyle(Color.fgPrimary)
                    VStack { Divider().background(Color.fgPrimary) }
                }

                testnetTopUpButton()
            #endif
        }
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
            DSButton(.actionSendCash, expands: true) {
                viewModel.onSendMoney?()
            }
            .accessibilityId(AccessibilityID.Wallet.sendPaymentButton)

            circleButton(.add24, isLoading: viewModel.isTopUpInProgress) {
                viewModel.onTopUp?()
            }
            .accessibilityId(AccessibilityID.Wallet.addFundsButton)

            withdrawButton()
        }
    }

    private func withdrawButton() -> some View {
        Button {
            viewModel.onWithdraw?()
        } label: {
            Group {
                if viewModel.isWithdrawInProgress {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .tint(.fgPrimaryInverted)
                } else {
                    Text(String(localized: .actionWithdraw))
                }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.ds(style: .primary, shape: .pill, size: .large))
        .disabled(viewModel.isWithdrawInProgress)
        .accessibilityId(AccessibilityID.Wallet.withdrawButton)
    }

    private func circleButton(
        _ icon: ImageResource,
        isLoading: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Group {
                if isLoading {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .tint(.fgPrimaryInverted)
                } else {
                    Image(icon)
                        .renderingMode(.template)
                }
            }
            .frame(width: 56, height: 56)
            .foregroundStyle(Color.fgPrimaryInverted)
            .background(.bgActionPrimary, in: Circle())
        }
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

/// Funding progress banner. Pinned by the wallet host while the asset card is expanded.
struct AssetDetailsFundingBar: View {
    @Bindable var viewModel: AssetDetailsViewModel

    var body: some View {
        if !viewModel.fundingStates.isEmpty {
            AssetFundingStatusView(
                states: $viewModel.fundingStates,
                isExpanded: $viewModel.isFundingExpanded,
                configuration: .fundingDigitalDollarConfiguration(
                    onCompletedAction: viewModel.onFundingCompleted,
                    onFailedAction: viewModel.onFundingFailed
                )
            )
            .frame(maxWidth: .infinity)
        }
    }
}

private struct CoinageBalanceBreakdownView: View {
    let breakdown: CoinageBalanceBreakdownViewModel

    @State private var showDetails = false
    /// How the coins came out, reported by the view as it lays them out.
    @State private var coinMetrics = CoinageCoinsView.Metrics()
    /// How tall the coins are actually drawn, as opposed to how much room the card gives them.
    @State private var drawHeight = CoinageStripLayout.Options().height
    /// Cancelled if the coins are asked to expand again before the last collapse has finished.
    @State private var shrink: Task<Void, Never>?

    /// Anchors the card for the scroll that follows it down as it collapses.
    private static let anchor = "coinageCard"
    /// How long the card takes to close. The coins' own springs are still settling for a moment
    /// after it, which is why giving the drawable back waits a little longer than this.
    private static let collapse: TimeInterval = 0.35

    var body: some View {
        // Vends a proxy for the wallet's own scroll view rather than making one: collapsing the
        // details shortens the page under the reader, and a scroll view already past the new bottom
        // has to be walked down to it rather than dropped there.
        ScrollViewReader { scroll in
            card(scroll: scroll)
        }
    }

    private func card(scroll: ScrollViewProxy) -> some View {
        VStack(spacing: DSSpacings.extraMedium) {
            VStack(spacing: 0) {
                Text(.coinageSummaryTitle)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityId(AccessibilityID.Wallet.coinageHeader)

                totalHeadline
            }

            // Nothing held, nothing to draw: no strip to reserve height for, no runs to rule, and
            // nothing for the toggle to expand.
            if !breakdown.strip.isEmpty {
                // Above the coins rather than below them, so it stays on the same side whether they
                // are stacked into the strip or spread out one by one.
                Button {
                    toggleDetails(scroll: scroll)
                } label: {
                    HStack(spacing: DSSpacings.extraSmall) {
                        Image(.iconArrowUp16)
                            .renderingMode(.template)
                            .rotationEffect(.degrees(showDetails ? 0 : 180))
                        Text(String(localized: showDetails ? .coinageHideDetails : .coinageShowDetails))
                            .typography(.bodyMediumEmphasized)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .foregroundStyle(.fgPrimary)
                }

                CoinageCoinsView(
                    coins: breakdown.strip,
                    isExpanded: showDetails,
                    metrics: $coinMetrics
                )
                // Tall enough for the roomiest arrangement it has been asked for, and never
                // resized while anything is animating. A Metal layer whose bounds change under an
                // animation keeps showing its last frame mapped into the new bounds, and a frame
                // of coins mapped anywhere is a frame of coins in the wrong place; collapsing made
                // the top row dart upwards and come back. Held at one size there is no mapping to
                // get wrong, and the card closes over the coins by clipping them.
                .frame(height: drawHeight, alignment: .top)
                .overlay(alignment: .topLeading) { blockHeaders }
                // What the card gives the coins, which is what animates. Expanding is deliberately
                // not animated: it takes its full height at once, so the coins fly out into a box
                // that is already the right size, and animating it made the scroll view chase a
                // growing content size and bounce against its own edge. Collapsing is animated,
                // from inside the toggle, where the scroll can be moved in the same breath.
                .frame(height: max(coinMetrics.height, CoinageStripLayout.Options().height), alignment: .top)
                .clipped()
                .onChange(of: coinMetrics.height) { _, height in
                    grow(to: height)
                }

                if !showDetails {
                    runMarkers
                        .transition(.opacity)
                }
            }
        }
        .padding(DSSpacings.mediumIncreased)
        .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
        .id(Self.anchor)
    }

    /// Grows the drawing surface to fit an arrangement, and gives the room back once the coins
    /// have settled into a smaller one.
    ///
    /// It cannot simply follow the card, because resizing a Metal layer while anything is
    /// animating shows the last frame mapped into the new bounds. Waiting until the movement is
    /// over leaves one resize with nothing animating over it, which is the case that has always
    /// been fine, and the surface stops holding a grid's worth of drawable for a strip.
    private func grow(to height: CGFloat) {
        shrink?.cancel()
        shrink = nil

        guard height < drawHeight else {
            drawHeight = max(drawHeight, height)
            return
        }

        shrink = Task { @MainActor in
            try? await Task.sleep(for: .seconds(Self.collapse + 0.1))

            guard !Task.isCancelled else { return }

            drawHeight = height
        }
    }

    /// Collapsing has to shorten the card and move the scroll view in one animation.
    ///
    /// The coins report their height only after laying out, which lands outside any transaction, so
    /// the card took the strip's height in a step of its own and the scroll view clamped to the new
    /// bottom in one jump. Taking the strip's height here, and asking the scroll view for the card
    /// in the same breath, makes the page shorten and the scroll follow it as one movement.
    private func toggleDetails(scroll: ScrollViewProxy) {
        let isCollapsing = showDetails

        withAnimation(.easeInOut(duration: Self.collapse)) {
            showDetails.toggle()

            guard isCollapsing else { return }

            coinMetrics.height = CoinageStripLayout.Options().height
            scroll.scrollTo(Self.anchor, anchor: .bottom)
        }
    }

    /// The Clearing and Ready headers over the grid. The layout leaves room for them above each
    /// block, so they sit in space the coins already made rather than pushing them about.
    @ViewBuilder
    private var blockHeaders: some View {
        ForEach(coinMetrics.blocks) { block in
            Text(String(localized: block.partition == .ready ? .coinageSpendable : .coinageLoading))
                .typography(.bodySmall)
                .foregroundStyle(Color.fgSecondary)
                .offset(y: block.top)
        }
    }

    private var totalHeadline: some View {
        DSAmount(
            amount: breakdown.totalBalance,
            symbol: breakdown.symbol,
            typography: .displaySmall
        )
        .lineLimit(1)
        .accessibilityId(AccessibilityID.Wallet.coinageTotalBalanceValue)
        .foregroundStyle(Color.fgPrimary)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Which coins are Clearing and which are Ready, ruled under the runs themselves.
    ///
    /// Only while the coins are in the strip: spread into the grid they have headers of their own,
    /// and the runs these rule no longer exist.
    @ViewBuilder
    private var runMarkers: some View {
        GeometryReader { proxy in
            CoinageRunMarkers(
                runs: coinMetrics.runs.compactMap { run in
                    CoinageRunMarkers.Run(
                        partition: run.partition,
                        start: run.start,
                        end: run.end,
                        title: String(localized: run.partition == .ready ? .coinageSpendable : .coinageLoading),
                        amount: run.partition == .ready
                            ? breakdown.availableNowBalance
                            : breakdown.gainingPrivacyBalance
                    )
                },
                width: proxy.size.width
            )
        }
        .frame(height: CoinageRunMarkers.height)
    }
}
