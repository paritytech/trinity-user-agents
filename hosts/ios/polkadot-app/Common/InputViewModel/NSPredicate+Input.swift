import Foundation

extension NSPredicate {
    static var notEmpty: NSPredicate {
        NSPredicate(format: "SELF != ''")
    }

    static func getUsername(for minLength: Int, maxLength: Int) -> NSPredicate {
        let format = "[a-z]{\(minLength),\(maxLength)}"
        return NSPredicate(format: "SELF MATCHES %@", format)
    }
}
