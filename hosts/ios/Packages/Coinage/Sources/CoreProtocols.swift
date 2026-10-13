import Foundation

public typealias PublicKey = Data
public typealias PrivateKey = Data
/// The item of a coin or voucher key inside its installation; stored as `Int64(bitPattern:)` in CoreData.
public typealias DerivationIndex = UInt64
