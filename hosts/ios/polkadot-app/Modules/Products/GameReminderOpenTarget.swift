import struct TrUAPIHost.ProductId
import Foundation
import KeyDerivation

/// The product a game reminder opens. Humanity books the reminder once it runs the weekly game in
/// its Polkadot room (individuality-apps#1281), but the game is still played in the game product,
/// so a reminder Humanity booked opens the game product of the same network. Any other booking
/// product opens itself, which is what the game product's own reminder always did.
///
/// Only opening changes: the reminder stays the booking product's, held, reported and cancelled
/// under its id.
enum GameReminderOpenTarget {
    static func product(toOpenFor bookedBy: ProductId) -> ProductId {
        let labels = bookedBy.rawValue.split(separator: ".")
        guard labels.count == 2, let tld = labels.last.map(String.init),
              (try? BuiltInProduct.personhood(for: tld)) == bookedBy,
              let game = try? BuiltInProduct.dim2(for: tld) else {
            return bookedBy
        }

        return game
    }
}
