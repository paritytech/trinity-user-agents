import Foundation

public enum ProductPageResolutionError: Error {
    case tldUnavailable(underlying: Error)
    case destinationNotOnNetwork(destination: String, tld: String)
}
