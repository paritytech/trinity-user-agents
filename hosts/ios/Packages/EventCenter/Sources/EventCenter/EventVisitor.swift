import Foundation

/// Marker protocol; concrete event-visitor methods are added via
/// extensions in the modules that emit the events (ChainRegistry, the
/// app target, etc.).
public protocol EventVisitorProtocol: AnyObject {}
