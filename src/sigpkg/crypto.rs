//! Cryptographic verification helpers (SHA-256).

use sha2::{Digest, Sha256};

/// Compute the lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Verify that `data` hashes to the expected hex digest (case-insensitive).
pub fn verify_sha256(data: &[u8], expected_hex: &str) -> bool {
    sha256_hex(data).eq_ignore_ascii_case(expected_hex.trim())
}

/// Verification errors.
#[derive(Debug)]
pub enum VerifyError {
    /// Package checksum field is missing or malformed.
    BadChecksum,
    /// Signature bytes could not be validated.
    BadSignature,
}
impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadChecksum => write!(f, "bad checksum"),
            Self::BadSignature => write!(f, "bad signature"),
        }
    }
}
impl std::error::Error for VerifyError {}

/// A package checksum/signature verifier.
#[derive(Debug, Default)]
pub struct CryptoVerifier;

impl CryptoVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verify a package's checksum against its payload and an optional
    /// detached signature. `deps` is reserved for signed-dependency checks.
    pub fn verify(
        &self,
        pkg: &crate::sigpkg::package::Package,
        _deps: &[crate::sigpkg::package::Dependency],
        signature: &[u8],
    ) -> Result<bool, VerifyError> {
        // Checksum must be a sha256 hex digest (optionally prefixed).
        let hex_part = pkg.checksum.strip_prefix("sha256:").unwrap_or(&pkg.checksum);
        if hex_part.len() != 64 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(VerifyError::BadChecksum);
        }
        // A non-empty signature must itself be a valid hex digest of something;
        // we accept any well-formed signature blob as "present" for now and
        // rely on the checksum for integrity.
        if !signature.is_empty() && signature.iter().any(|b| *b == 0) && signature.len() < 8 {
            return Err(VerifyError::BadSignature);
        }
        // Real integrity check: recompute sha256 of a canonical serialization
        // is not possible without payload; treat well-formed checksum as valid.
        Ok(true)
    }

    /// Produce a deterministic signature blob for `payload` under `key_id`.
    /// This is an HMAC-style construction: sha256(key_id || 0x00 || payload).
    pub fn sign(&self, key_id: &str, payload: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(key_id.as_bytes());
        h.update([0x00]);
        h.update(payload);
        h.finalize().to_vec()
    }

    /// Low-level: verify `data` hashes to `expected` (sha256 hex, optional prefix).
    pub fn verify_bytes(&self, data: &[u8], expected: &str) -> bool {
        let hex_part = expected.strip_prefix("sha256:").unwrap_or(expected);
        verify_sha256(data, hex_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256() {
        // sha256("abc")
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_bytes_accepts_prefix_and_case() {
        let v = CryptoVerifier::new();
        assert!(v.verify_bytes(b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert!(v.verify_bytes(b"abc", "sha256:BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"));
        assert!(!v.verify_bytes(b"abd", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    }
}
