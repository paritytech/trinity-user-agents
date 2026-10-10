import UIKit
import DesignSystem

/// One open product as the shelf shows it.
public struct DSTabBarChip {
    public let id: UUID
    public let name: String
    public let icon: ImageViewModelProtocol?

    public init(id: UUID, name: String, icon: ImageViewModelProtocol?) {
        self.id = id
        self.name = name
        self.icon = icon
    }
}
