import Observation
import SwiftUI

@Observable
public final class AppsListViewModel {
    public var items: [AppsListViewLayout.Item] = []
    public var onSelect: ((AppsListViewLayout.Item) -> Void)?

    public init() {}
}
