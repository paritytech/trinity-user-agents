import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI

/// The coinage card under the balance card: the total, the coins as a strip or a grid, and the
/// rules under the strip's runs.
struct CoinageBalanceBreakdownView: View {
    let breakdown: CoinageBalanceBreakdownViewModel
    /// Which coins the denominations are drawn as. Only the debug switch ever changes it.
    var coinSet: CoinageScene.CoinSet = .shipped

    @State private var showDetails = false
    /// The coin picked out by a tap, if any, drawn large with its trace beside it.
    @State private var focusedCoin: String?
    /// Whether the picked coin's quality is being explained.
    @State private var isExplainingQuality = false
    /// How the coins came out, reported by the view as it lays them out.
    @State private var coinMetrics = CoinageCoinsView.Metrics()
    /// How tall the coins are actually drawn, as opposed to how much room the card gives them.
    @State private var drawHeight = CoinageStripLayout.Options().height

    /// Anchors the card for the scroll that follows it down as it collapses.
    private static let anchor = "coinageCard"
    /// How long the card takes to close. The coins' own springs settle on their own time after
    /// it, which is why the drawing surface waits for them rather than for this.
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
            // Spread out, the coins gather back on a tap here, on the card's own heading, and
            // never on a tap among them: a finger meant for a coin must not close the grid.
            .contentShape(Rectangle())
            .onTapGesture {
                if showDetails { toggleDetails(scroll: scroll) }
            }

            // Nothing held, nothing to draw: no strip to reserve height for, no runs to rule, and
            // nothing to unfurl.
            if !breakdown.strip.isEmpty {
                // The coins are their own control: a tap or a flick down on the strip unfurls
                // them, a flick up on the grid gathers them back, and a tap on one of the spread
                // coins picks it out. There is no label saying so, by request.
                coins(scroll: scroll)
                    // Tall enough for the roomiest arrangement it has been asked for, and never
                    // resized while anything is animating. A Metal layer whose bounds change under an
                    // animation keeps showing its last frame mapped into the new bounds, and a frame
                    // of coins mapped anywhere is a frame of coins in the wrong place; collapsing made
                    // the top row dart upwards and come back. Held at one size there is no mapping to
                    // get wrong, and the card closes over the coins by clipping them.
                    .frame(height: drawHeight, alignment: .top)
                    // Behind the coins, not over them: the surface is clear wherever no coin is
                    // drawn, so the headers show through the gaps the layout left for them, and a
                    // picked coin flying across a header passes in front of it.
                    .background(alignment: .topLeading) { blockHeaders }
                    .overlay(alignment: .topLeading) { tracePanel }
                    // What the card gives the coins, which is what animates. Expanding is deliberately
                    // not animated: it takes its full height at once, so the coins fly out into a box
                    // that is already the right size, and animating it made the scroll view chase a
                    // growing content size and bounce against its own edge. Collapsing is animated,
                    // from inside the toggle, where the scroll can be moved in the same breath.
                    .frame(height: max(coinMetrics.height, CoinageStripLayout.Options().height), alignment: .top)
                    .clipped()
                    .onChange(of: coinMetrics) { _, metrics in
                        fit(metrics)
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

    /// Grows the drawing surface to fit an arrangement at once, and gives the room back only once
    /// the coins report that they have settled into a smaller one.
    ///
    /// It cannot simply follow the card, because resizing a Metal layer while anything is
    /// animating shows the last frame mapped into the new bounds. A timer was tried and could
    /// only guess at the springs; it fired just before the coins arrived, which was a visible
    /// stretch and a hitch every time. The coins know when they are at rest, so they say so.
    private func fit(_ metrics: CoinageCoinsView.Metrics) {
        if metrics.height >= drawHeight {
            drawHeight = metrics.height
        } else if !metrics.isMoving {
            drawHeight = metrics.height
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
            focusedCoin = nil
            isExplainingQuality = false

            guard isCollapsing else { return }

            // On their way as of now, so the surface is not given back before they report in.
            coinMetrics.isMoving = true
            coinMetrics.height = CoinageStripLayout.Options().height
            scroll.scrollTo(Self.anchor, anchor: .bottom)
        }
    }

    /// The coins, as a view as tall as the field. Inside it the Metal surface is only a screen
    /// tall and follows the scroll, which the view manages itself.
    private func coins(scroll: ScrollViewProxy) -> some View {
        CoinageCoinsView(
            coins: breakdown.strip,
            isExpanded: showDetails,
            focus: focus,
            coinSet: coinSet,
            metrics: $coinMetrics,
            onTap: { tapped($0, scroll: scroll) },
            onSwipe: { swiped($0, scroll: scroll) }
        )
    }

    /// The picked coin and the room its trace needs, or nothing while no coin is picked or the
    /// picked one has since left the holdings.
    private var focus: CoinageArrangement.Focus? {
        guard let focusedCoin, let trace = breakdown.traces[focusedCoin],
              breakdown.strip.contains(where: { $0.id == focusedCoin })
        else {
            return nil
        }

        return CoinageArrangement.Focus(
            id: focusedCoin,
            band: CoinageTraceView.height(for: trace)
        )
    }

    /// In the strip any tap unfurls the coins: they are too small and too close to pick one out
    /// of, and unfurling is what a tap on a stack of coins asks for. Spread out, a tap picks a
    /// coin, and a tap on the picked one or on empty ground puts it back.
    private func tapped(_ coin: String?, scroll: ScrollViewProxy) {
        guard showDetails else {
            toggleDetails(scroll: scroll)
            return
        }

        // Spread out: a coin picks it, and the picked coin or empty ground puts it back. Nothing
        // among the coins gathers them: that is the card's heading, where a finger cannot miss a
        // coin and close the grid instead.
        withAnimation(.easeInOut(duration: Self.collapse)) {
            if let coin, coin != focusedCoin {
                focusedCoin = coin
                isExplainingQuality = false
            } else {
                unfocus()
            }
        }
    }

    private func swiped(_ swipe: CoinageCoinsView.Swipe, scroll: ScrollViewProxy) {
        if swipe == .downward, !showDetails {
            toggleDetails(scroll: scroll)
        }
    }

    /// Takes the band's height back in the same breath as the coin leaves it, for the same reason
    /// collapsing does: the coins report their height only after laying out, which would shorten
    /// the card in a step of its own.
    private func unfocus() {
        guard let focus else { return }

        focusedCoin = nil
        isExplainingQuality = false
        coinMetrics.isMoving = true

        // Only a band that sat below the coins made the field taller; one floating over them did
        // not, and the height must not be taken from the coins it is still covering.
        if let top = coinMetrics.focusTop, top + focus.band >= coinMetrics.height {
            coinMetrics.height = max(top, CoinageStripLayout.Options().height)
        }
    }

    /// What the picked coin says about itself, beside it in the band.
    @ViewBuilder
    private var tracePanel: some View {
        if let focus, let trace = breakdown.traces[focus.id] {
            CoinageTraceView(trace: trace, isExplainingQuality: $isExplainingQuality)
                .padding(.horizontal, DSSpacings.small)
                // The box wraps what it holds and grows downward while the quality is being
                // explained. The band it sits in is placed for the box at its tallest, so the box
                // never has to move; it only reaches further down, and never off the screen.
                .background(.bgSurfaceContainer.opacity(0.92), in: RoundedRectangle(cornerRadius: DSRadii.medium))
                .animation(.easeInOut(duration: 0.2), value: isExplainingQuality)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, CoinageArrangement.focusDiameter + CoinageArrangement.focusInset * 2)
                .offset(y: coinMetrics.focusTop ?? 0)
                .transition(.opacity)
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
