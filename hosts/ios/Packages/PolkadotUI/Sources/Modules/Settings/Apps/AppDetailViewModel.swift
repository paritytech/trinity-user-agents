import Observation
import SwiftUI

@Observable
public final class AppDetailViewModel {
    public var name: String = ""
    /// The product's domain, shown only when the manifest gave it a different display name.
    public var subtitle: String?
    /// Shown until ``icon`` loads, and for products that never resolve one.
    public var avatar: AvatarViewModel = .colored(text: "", colorSeed: "")
    public var icon: (any ImageViewModelProtocol)?
    public var onPermissionsTap: (() -> Void)?

    public init() {}
}
