import Observation
import SwiftUI

public struct IdentityDetailsUsernameViewModel: Equatable {
    public let value: String
    let isClaimed: Bool

    public init(value: String, isClaimed: Bool) {
        self.value = value
        self.isClaimed = isClaimed
    }
}

public protocol IdentityDetailsViewModelProtocol: Observation.Observable {
    var username: IdentityDetailsUsernameViewModel? { get set }
    var qrCode: Image? { get set }
    var onCopy: (() -> Void)? { get set }
    var onShare: (() -> Void)? { get set }
    var onQrCode: (() -> Void)? { get set }
    /// Rank reflects personhood state, which not every build can reach. Hosts that cannot
    /// resolve it set this to false so the card omits the rank instead of showing "Basic".
    var isRankVisible: Bool { get set }
}

@Observable
public class IdentityDetailsViewModel: IdentityDetailsViewModelProtocol {
    public var username: IdentityDetailsUsernameViewModel?
    public var qrCode: Image?
    public var onCopy: (() -> Void)?
    public var onShare: (() -> Void)?
    public var onQrCode: (() -> Void)?
    public var isRankVisible: Bool = true

    public init() {}
}
