import Foundation
import SubstrateSdk
import SubstrateSdkExt

public enum CustomSiMappers {
    public static var all: SiTypeMapping {
        OneOfSiTypeMapper(innerMappers: [
            SiDataTypeMapper()
        ])
    }
}
