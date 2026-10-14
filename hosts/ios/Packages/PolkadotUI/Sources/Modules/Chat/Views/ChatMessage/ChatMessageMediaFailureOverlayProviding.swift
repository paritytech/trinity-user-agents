public protocol ChatMessageMediaFailureOverlayProviding: AnyObject {
    func startUpdate(onUpdate: @escaping (_ isFailureVisible: Bool) -> Void)
    func stopUpdate()
}
