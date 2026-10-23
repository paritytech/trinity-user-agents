import SwiftUI
import Testing
@testable import polkadot_app

@MainActor
struct WalletPocketOpenCardTests {
    /// A card's screen hides its own card while it flies, so the Pocket is what
    /// draws the card on its way. A Pocket that hid its open card would leave a
    /// gap where the card should move.
    @Test
    func keepsDrawingTheOpenCardWhereItSits() throws {
        let wallet = WalletView()
        wallet.viewModel.pocketCards = [loyaltyCard]
        let screen = CGSize(width: 393, height: 852)

        let closed = try WidgetNodeRaster.pixels(of: wallet, in: screen, under: .berlinNight)
        wallet.viewModel.openPocketCard = loyaltyCard.key
        let open = try WidgetNodeRaster.pixels(of: wallet, in: screen, under: .berlinNight)

        let differingBytes = zip(open, closed).filter { $0 != $1 }.count
        #expect(differingBytes == 0)
    }
}
