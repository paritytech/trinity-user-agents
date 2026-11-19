import Observation
import SwiftUI

@Observable
public final class AppsListViewModel<Identifier: Hashable> {
    public var items: [AppsListViewLayout<Identifier>.Item] = []
    public var onSelect: ((AppsListViewLayout<Identifier>.Item) -> Void)?

    public init() {}
}
