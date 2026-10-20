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
            #if DEBUG
                // The card stays on a wallet that holds no coinage, which is where the test
                // sets are most useful.
                if viewModel.balanceCardModel != nil {
                    CoinageBalanceBreakdownView(
                        breakdown: testDataMode.applied(to: viewModel.coinageBreakdown ?? .testDataPlaceholder),
                        coinSet: coinSet
                    )
                }
            #else
                if let breakdown = viewModel.coinageBreakdown,
                   viewModel.balanceCardModel != nil {
                    CoinageBalanceBreakdownView(breakdown: breakdown)
                }
            #endif

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
            getButton()
            sendButton()
        }
    }

    private func getButton() -> some View {
        Button {
            viewModel.onTopUp?()
        } label: {
            Group {
                if viewModel.isTopUpInProgress {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .tint(.fgPrimaryInverted)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    actionLabel(
                        icon: .iconArrowUp24,
                        rotation: .zero,
                        title: .actionGet,
                        subtitle: .actionGetSubtitle
                    )
                }
            }
        }
        .buttonStyle(.ds(style: .primary, shape: .rounded, size: .large))
        .disabled(viewModel.isTopUpInProgress)
        .accessibilityId(AccessibilityID.Wallet.addFundsButton)
    }

    private func sendButton() -> some View {
        Button {
            viewModel.onSendMoney?()
        } label: {
            actionLabel(
                icon: .iconArrowUp24,
                rotation: .degrees(180),
                title: .actionSendCash,
                subtitle: .actionSendSubtitle
            )
        }
        .buttonStyle(.ds(style: .primary, shape: .rounded, size: .large))
        .accessibilityId(AccessibilityID.Wallet.sendPaymentButton)
    }

    /// Two rows: icon + title, then subtitle. `maxHeight: .infinity` lets the shorter
    /// button match the taller one so the pair reads as one row.
    private func actionLabel(
        icon: ImageResource,
        rotation: Angle,
        title: LocalizedStringResource,
        subtitle: LocalizedStringResource
    ) -> some View {
        VStack(spacing: DSSpacings.tiny) {
            HStack(spacing: DSSpacings.extraSmall) {
                Image(icon)
                    .renderingMode(.template)
                    .rotationEffect(rotation)
                Text(title)
                    .typography(.titleLarge)
            }
            Text(subtitle)
                .typography(.bodySmall)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
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
