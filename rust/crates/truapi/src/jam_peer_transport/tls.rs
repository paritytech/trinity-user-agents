//! JAMNP-S TLS: a self-signed Ed25519 client certificate whose single DNS
//! alternative name is the key's text form, and a server verifier that pins
//! the peer's Ed25519 key. Certificate signatures are not checked (the spec
//! neither requires nor forbids self-signing); the TLS 1.3 handshake signature
//! is verified against the pinned key.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use x509_parser::der_parser::asn1_rs::oid;
use x509_parser::prelude::*;

use super::peer_id;

/// Failure to build the local identity.
#[derive(Debug, thiserror::Error)]
#[error("cannot build the local JAMNP-S identity: {0}")]
pub(super) struct IdentityError(#[from] rcgen::Error);

/// Local Ed25519 identity with its self-signed certificate.
pub(super) struct Identity {
    public: [u8; 32],
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
}

impl Identity {
    /// A fresh identity; every peer accepts any well-formed key.
    pub(super) fn generate() -> Result<Self, IdentityError> {
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
        let public: [u8; 32] = key_pair
            .public_key_raw()
            .try_into()
            .expect("Ed25519 public keys are 32 bytes");
        let mut params = rcgen::CertificateParams::new(vec![peer_id::ed25519_text(&public)])?;
        let mut name = rcgen::DistinguishedName::new();
        name.push(rcgen::DnType::CommonName, "jam");
        params.distinguished_name = name;
        let cert = params.self_signed(&key_pair)?;
        Ok(Self {
            public,
            cert: cert.der().clone(),
            key: PrivatePkcs8KeyDer::from(key_pair.serialize_der()),
        })
    }

    /// Our Ed25519 public key.
    pub(super) fn public(&self) -> &[u8; 32] {
        &self.public
    }

    pub(super) fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![self.cert.clone()]
    }

    pub(super) fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key.clone_key())
    }
}

const BAD_ENCODING: rustls::Error =
    rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding);
const APP_VERIF_FAILURE: rustls::Error =
    rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure);
const NOT_VALID_FOR_NAME: rustls::Error =
    rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName);
const BAD_SIGNATURE: rustls::Error =
    rustls::Error::InvalidCertificate(rustls::CertificateError::BadSignature);

/// Ed25519 key of a JAMNP-S certificate, after checking its form: Ed25519
/// SPKI, at most V3, one DNS alternative name equal to the key's text form,
/// no unknown critical extensions.
fn peer_key(cert: &CertificateDer<'_>) -> Result<[u8; 32], rustls::Error> {
    let (rest, cert) = X509Certificate::from_der(cert).map_err(|_| BAD_ENCODING)?;
    if !rest.is_empty() || cert.version.0 > 2 {
        return Err(BAD_ENCODING);
    }
    let spki = &cert.subject_pki;
    if spki.algorithm.algorithm != oid!(1.3.101.112) || spki.algorithm.parameters.is_some() {
        return Err(APP_VERIF_FAILURE);
    }
    if spki.subject_public_key.unused_bits != 0 {
        return Err(BAD_ENCODING);
    }
    let key: [u8; 32] = spki
        .subject_public_key
        .as_ref()
        .try_into()
        .map_err(|_| BAD_ENCODING)?;
    let mut alternative_names = None;
    for extension in cert.extensions() {
        match extension.parsed_extension() {
            ParsedExtension::SubjectAlternativeName(names) => {
                if alternative_names.replace(names).is_some() {
                    return Err(BAD_ENCODING);
                }
            }
            ParsedExtension::ParseError { .. } => return Err(BAD_ENCODING),
            _ if extension.critical => {
                return Err(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::UnhandledCriticalExtension,
                ));
            }
            _ => {}
        }
    }
    let names = &alternative_names.ok_or(APP_VERIF_FAILURE)?.general_names;
    let [GeneralName::DNSName(name)] = names.as_slice() else {
        return Err(APP_VERIF_FAILURE);
    };
    if *name != peer_id::ed25519_text(&key) {
        return Err(APP_VERIF_FAILURE);
    }
    Ok(key)
}

/// Pins one expected Ed25519 peer key for a single dial.
#[derive(Debug)]
struct PinnedPeer {
    expected: [u8; 32],
}

impl ServerCertVerifier for PinnedPeer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let key = peer_key(end_entity)?;
        if key != self.expected {
            return Err(APP_VERIF_FAILURE);
        }
        let ServerName::DnsName(name) = server_name else {
            return Err(NOT_VALID_FOR_NAME);
        };
        if name.as_ref() != peer_id::ed25519_text(&key) {
            return Err(NOT_VALID_FOR_NAME);
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::Error::PeerMisbehaved(
                rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
            ));
        }
        let key = peer_key(cert)?;
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
            .verify(message, dss.signature())
            .map_err(|_| BAD_SIGNATURE)?;
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

/// rustls client configuration for one dial: TLS 1.3, ring, our certificate,
/// the pinned peer verifier and the chain's ALPN.
pub(super) fn client_config(
    identity: &Identity,
    expected: [u8; 32],
    alpn: Vec<u8>,
) -> Result<rustls::ClientConfig, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("ring supports TLS 1.3")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedPeer { expected }))
        .with_client_auth_cert(identity.cert_chain(), identity.private_key())?;
    config.alpn_protocols = vec![alpn];
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dial names one key; any other certificate, or the right certificate
    /// under another name, must fail the handshake.
    #[test]
    fn only_the_pinned_key_under_its_own_name_passes() {
        let identity = Identity::generate().unwrap();
        let other = Identity::generate().unwrap();
        let name = |identity: &Identity| {
            ServerName::try_from(peer_id::ed25519_text(identity.public())).unwrap()
        };
        let verify = |expected: &Identity, server_name: &ServerName<'_>| {
            PinnedPeer {
                expected: *expected.public(),
            }
            .verify_server_cert(&identity.cert, &[], server_name, &[], UnixTime::now())
            .is_ok()
        };

        assert_eq!(peer_key(&identity.cert).unwrap(), *identity.public());
        assert_eq!(
            [
                verify(&identity, &name(&identity)),
                verify(&other, &name(&identity)),
                verify(&identity, &name(&other)),
            ],
            [true, false, false],
        );
    }
}
