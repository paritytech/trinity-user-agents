import Observation
import SwiftUI

@Observable
public class BlockedUsersViewModel {
    public var items: [BlockedUsersViewLayout.Item] = []
    public var onSelect: ((BlockedUsersViewLayout.Item) -> Void)?
    public var onUnblock: ((BlockedUsersViewLayout.Item) -> Void)?

    public init() {}
}
