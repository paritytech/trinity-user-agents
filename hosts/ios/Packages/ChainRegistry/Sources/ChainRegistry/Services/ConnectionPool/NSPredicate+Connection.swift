import Foundation

public extension NSPredicate {
    static var wsProtocol: NSPredicate { websocketPredicate(for: .ws) }

    static var ipUrlPredicate: NSPredicate {
        let urlRegEx = "^(https?://)?((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.){3}" +
            "(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?):[0-9]+(/[-\\w@\\+\\.~#\\?&/=%]*)?$"
        return NSPredicate(format: "SELF MATCHES %@", urlRegEx)
    }

    static var domainUrlPredicate: NSPredicate {
        let urlRegEx = "^(https?://)?(www\\.)?([-a-z0-9]{1,63}\\.)*?" +
            "[a-z0-9][-a-z0-9]{0,61}[a-z0-9]\\.[a-z]{2,16}" +
            "(/[-\\w@\\+\\.~#\\?&/=%]*)?$"
        return NSPredicate(format: "SELF MATCHES %@", urlRegEx)
    }

    static var urlPredicate: NSPredicate {
        NSCompoundPredicate(orPredicateWithSubpredicates: [
            domainUrlPredicate,
            ipUrlPredicate
        ])
    }

    private static func websocketPredicate(for scheme: WebsocketScheme) -> NSPredicate {
        // protocol identifier (optional)
        // short syntax // still required
        let schemeRegExp =
            switch scheme {
            case .ws:
                "(?:(?:(?:wss?|ws):)?\\/\\/)"
            case .wss:
                "(?:(?:(?:wss?):)?\\/\\/)"
            }

        let format = "^" +
            schemeRegExp +
            // user:pass BasicAuth (optional)
            "(?:\\S+(?::\\S*)?@)?" +
            "(?:" +
            // IP address exclusion
            // private & local networks
            "(?!(?:10|127)(?:\\.\\d{1,3}){3})" +
            "(?!(?:169\\.254|192\\.168)(?:\\.\\d{1,3}){2})" +
            "(?!172\\.(?:1[6-9]|2\\d|3[0-1])(?:\\.\\d{1,3}){2})" +
            // IP address dotted notation octets
            // excludes loopback network 0.0.0.0
            // excludes reserved space >= 224.0.0.0
            // excludes network & broadcast addresses
            // (first & last IP address of each class)
            "(?:[1-9]\\d?|1\\d\\d|2[01]\\d|22[0-3])" +
            "(?:\\.(?:1?\\d{1,2}|2[0-4]\\d|25[0-5])){2}" +
            "(?:\\.(?:[1-9]\\d?|1\\d\\d|2[0-4]\\d|25[0-4]))" +
            "|" +
            // host & domain names, may end with dot
            // can be replaced by a shortest alternative
            // (?![-_])(?:[-\\w\\u00a1-\\uffff]{0,63}[^-_]\\.)+
            "(?:" +
            "(?:" +
            "[a-z0-9\\u00a1-\\uffff]" +
            "[a-z0-9\\u00a1-\\uffff_-]{0,62}" +
            ")?" +
            "[a-z0-9\\u00a1-\\uffff_-]\\." +
            ")+" +
            // TLD identifier name, may end with dot
            "(?:[a-z\\u00a1-\\uffff]{2,}\\.?)" +
            ")" +
            // port number (optional)
            "(?::\\d{2,5})?" +
            // resource path (optional)
            "(?:[/?#]\\S*)?" +
            "$"

        return NSPredicate(format: "SELF MATCHES %@", format)
    }

    private enum WebsocketScheme: String {
        // swiftlint:disable:next identifier_name
        case ws
        case wss
    }
}
