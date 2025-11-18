//! Certificate management with ACME support
//!
//! Handles automatic certificate provisioning via Let's Encrypt/ACME,
//! certificate storage, and auto-renewal.

pub mod acme;
pub mod self_signed;
pub mod storage;

pub use acme::{AcmeClient, AcmeConfig, AcmeError, Http01Challenge, Http01ChallengeCallback};
pub use self_signed::{
    generate_self_signed_cert, generate_self_signed_cert_with_domains, SelfSignedCertificate,
    SelfSignedError,
};
pub use storage::{CertificateStore, StoredCertificate};

use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Certificate with private key
#[derive(Debug)]
pub struct Certificate {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub private_key: PrivateKeyDer<'static>,
}

impl Certificate {
    pub fn new(
        cert_chain: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Self {
        Self {
            cert_chain,
            private_key,
        }
    }
}
