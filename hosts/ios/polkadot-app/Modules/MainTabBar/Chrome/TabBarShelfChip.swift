import Foundation
import PolkadotUI

/// One open product as the shelf shows it.
struct TabBarShelfChip: Identifiable, Equatable {
    let id: UUID
    let name: String
    let icon: AnyImageViewModel?
}
