import Observation
import SwiftUI

@Observable
public final class AppPermissionsViewModel {
    public var items: [AppPermissionsViewLayout.Item] = []
    public var onToggle: ((AppPermissionsViewLayout.Item, Bool) -> Void)?

    public init() {}
}
