import Foundation
@testable import polkadot_app
import PolkadotUI
import SwiftUI
import Testing
import TrUAPIHost
import UIKit

/// Draws the open Purse at fixed scroll offsets and writes each frame to the
/// folder `PURSE_SNAPSHOT_DIR` names, for checking by eye that the card docks
/// in the header and that the page reads card, tiles, Total, history. Skipped
/// unless that variable is set (as `TEST_RUNNER_PURSE_SNAPSHOT_DIR` through
/// xcodebuild).
@MainActor
struct PurseSnapshotTests {
    nonisolated static let directory = ProcessInfo.processInfo.environment["PURSE_SNAPSHOT_DIR"]
    static let offsets: [CGFloat] = [0, 75, 200]

    @Test(.enabled(if: directory != nil))
    func drawsTheOpenPurseAtEachOffset() async throws {
        let folder = try URL(fileURLWithPath: #require(Self.directory))
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let center = FundingActivityCenter(store: SnapshotHistoryStore(rows: Self.records(now: Date())))
        center.refresh()
        let window = try openPurse(center: center)
        defer { window.isHidden = true }
        try await Task.sleep(for: .seconds(1.5))
        let scroll = try #require(Self.scrollView(in: window))

        for offset in Self.offsets {
            scroll.setContentOffset(CGPoint(x: 0, y: offset - scroll.adjustedContentInset.top), animated: false)
            try await Task.sleep(for: .seconds(1))
            let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }
            let file = folder.appendingPathComponent("purse-offset-\(Int(offset)).png")
            try #require(image.pngData()).write(to: file)
        }
    }
}

private extension PurseSnapshotTests {
    func openPurse(center: FundingActivityCenter) throws -> UIWindow {
        let assets = AssetDetailsViewModel()
        assets.balanceCardModel = AssetDetailsBalanceCard.ViewModel(
            balance: "$424.13",
            readyBalance: "$300",
            symbol: "CASH"
        )
        assets.coinageBreakdown = CoinageBalanceBreakdownViewModel(
            totalBalance: "$424.13",
            availableNowBalance: "$300",
            gainingPrivacyBalance: "$124.13",
            symbol: "CASH",
            amounts: CoinageAmounts(total: 424.13, availableNow: 300, gainingPrivacy: 124.13),
            strip: Self.coins
        )
        let wallet = WalletViewModel()
        wallet.assetDetailsViewModel = assets
        wallet.expandedSection = .assetDetails

        let host = UIHostingController(rootView: WalletView(
            viewModel: wallet,
            fundingCenter: center,
            cash: FundingCash(symbol: "USDC", precision: 6)
        ))
        host.view.backgroundColor = .bgSurfaceMain
        let title = PurseTitleView()
        host.navigationItem.titleView = title
        wallet.onPurseCollapse = { title.apply(progress: $0) }

        let scene = try #require(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        window.windowLevel = .alert + 1
        window.rootViewController = UINavigationController(rootViewController: host)
        window.makeKeyAndVisible()
        return window
    }

    static let coins: [CoinageScene.Coin] = (0 ..< 9).map { index in
        CoinageScene.Coin(
            id: "coin-\(index)",
            exponent: Int16(index % 4 + 4),
            wear: 0,
            partition: index < 3 ? .clearing : .ready,
            level: 0,
            hops: 0
        )
    }

    static func records(now: Date) -> [FundingRecord] {
        let rows: [(String, FundingDirection, FundingRail, String, TimeInterval)] = [
            ("fs_snapshot_1", .in, .card, "25000000", 600),
            ("fs_snapshot_2", .out, .crypto, "50000000", 1_800),
            ("fs_snapshot_3", .in, .bank, "125250000", 90_000),
            ("fs_snapshot_4", .out, .card, "40000000", 100_000)
        ]
        return rows.map { intent, direction, rail, amount, ago in
            FundingRecord(
                intent: intent,
                direction: direction,
                rail: rail,
                asset: nil,
                providerId: nil,
                requestedAmount: amount,
                settledAmount: amount,
                paidAmount: nil,
                paidAsset: nil,
                paidDecimals: nil,
                paidNetwork: nil,
                outcome: direction == .in ? .delivered : .released,
                payout: nil,
                refundedAmount: nil,
                transactionId: nil,
                reference: nil,
                openedAt: now - ago,
                settledAt: now - ago
            )
        }
    }

    static func scrollView(in view: UIView) -> UIScrollView? {
        if let scroll = view as? UIScrollView { return scroll }
        for subview in view.subviews {
            if let found = scrollView(in: subview) { return found }
        }
        return nil
    }
}

/// History for the snapshot, held in memory: the app's own store needs the
/// shared container a test host does not have.
private struct SnapshotHistoryStore: FundingHistoryStoring {
    let rows: [FundingRecord]

    func records() async throws -> [FundingRecord] {
        rows
    }

    func save(_: FundingRecord) async throws {}
}
