import Foundation
import Individuality
import KeyDerivation

struct CreateProofOrAliasOption {
    let collectionId: MembersPallet.CollectionIdentifier
    let keyManager: BandersnatchKeyManaging
}
