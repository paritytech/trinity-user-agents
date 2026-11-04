import struct TrUAPIHost.ProductId
import Testing
@testable import polkadot_app

@Suite("The product a game reminder opens")
struct GameReminderOpenTargetTests {
    /// Humanity runs the game in its Polkadot room (individuality-apps#1281), but the game is
    /// played in the game product of the same network.
    @Test(
        "Humanity's reminder opens the game product of its own network",
        arguments: [("peopl.dot", "dim2.dot"), ("peopl.paseo", "dim2.paseo"), ("peopl.testnet", "dim2.testnet")]
    )
    func humanityOpensTheGame(bookedBy: String, opens: String) throws {
        #expect(try GameReminderOpenTarget.product(toOpenFor: ProductId(bookedBy)) == ProductId(opens))
    }

    /// The game product's own reminder, and any lookalike of Humanity, open the booking product
    /// as before.
    @Test(
        "Any other booking product opens itself",
        arguments: ["dim2.dot", "dim2.paseo", "app.peopl.paseo", "peoplx.dot", "game.dot"]
    )
    func otherProductsOpenThemselves(bookedBy: String) throws {
        let productId = try ProductId(bookedBy)

        #expect(GameReminderOpenTarget.product(toOpenFor: productId) == productId)
    }
}
