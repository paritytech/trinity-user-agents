import Foundation
import Observation
import TrUAPIHost
import UIKitExt

/// Which sessions a list shows.
enum FundingActivityMode: Equatable {
    /// Every session, in flight and ended.
    case history
    /// Those in flight in one direction, before another is started.
    case pending(FundingDirection)
}

/// What a session list draws.
struct FundingActivityState: Equatable {
    var inFlight: [FundingActivityItem] = []
    var history: [FundingActivityItem] = []
    /// Rows that just finished, which arrive with their completion.
    var arriving: Set<String> = []
}

@MainActor
@Observable
final class FundingActivityViewModel {
    var state = FundingActivityState()
}

@MainActor
protocol FundingActivityViewProtocol: ControllerBackedProtocol {
    func didReceive(state: FundingActivityState)
}

@MainActor
protocol FundingActivityPresenterProtocol: AnyObject {
    func setup()
    func open(_ item: FundingActivityItem)
    func showHistory()
    func startNew()
    func back()
}

@MainActor
protocol FundingActivityInteractorInputProtocol: AnyObject {
    /// Follows every session; with `refreshing`, reads them again every few
    /// seconds, so a step reached without a status change still shows.
    func setup(refreshing: Bool)
    func acknowledge(_ entryId: String)
}

@MainActor
protocol FundingActivityInteractorOutputProtocol: AnyObject {
    func didReceive(entries: [FundingEntry])
}

@MainActor
protocol FundingActivityWireframeProtocol: AnyObject {
    func open(entryId: String)
    func showHistory()
    func startNew(direction: FundingDirection)
    func back()
}
