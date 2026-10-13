//! Versioned wrappers for [`ExpandedCard`](crate::api::ExpandedCard) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostExpandedCardSetFaceShownRequest { V1 => v01::HostExpandedCardSetFaceShownRequest }
    pub enum HostExpandedCardSetFaceShownResponse { V1 }
    pub enum HostExpandedCardSetFaceShownError { V1 => v01::HostExpandedCardSetFaceShownError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;

    // A Widget kept loaded after its card closed gets this error, and its
    // discriminant must stay first so older clients keep decoding it.
    #[test]
    fn not_presented_error_is_discriminant_zero() {
        let error = HostExpandedCardSetFaceShownError::V1(
            v01::HostExpandedCardSetFaceShownError::NotPresented,
        );

        assert_eq!(hex::encode(error.encode()), "0000");
    }
}
