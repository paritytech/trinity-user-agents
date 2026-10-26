import Observation
import PolkadotUI

/// Live chain connection rows shared between the presenter, which receives every update, and the
/// connection status sheet, which renders whichever rows are current while it is on screen.
@MainActor
@Observable
final class ConnectionStatusSheetViewModel {
    var rows: [ChainConnectionStatusViewModel] = []
}
