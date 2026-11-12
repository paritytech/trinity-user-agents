import AVFoundation

protocol RecordPermissionProviding: AnyObject {
    var recordPermission: AVAudioApplication.recordPermission { get }
}

protocol RecordPermissionRequesting: AnyObject {
    func requestRecordPermission() async -> Bool
}

final class RecordPermissionService {}

extension RecordPermissionService: RecordPermissionProviding {
    var recordPermission: AVAudioApplication.recordPermission {
        AVAudioApplication.shared.recordPermission
    }
}

extension RecordPermissionService: RecordPermissionRequesting {
    func requestRecordPermission() async -> Bool {
        await AVAudioApplication.requestRecordPermission()
    }
}
