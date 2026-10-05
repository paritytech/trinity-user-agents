@testable import polkadot_app
import AVFoundation

final class StubRecordPermissionProvider: RecordPermissionProviding {
    let recordPermission: AVAudioApplication.recordPermission

    init(recordPermission: AVAudioApplication.recordPermission) {
        self.recordPermission = recordPermission
    }
}
