//! OpenSSH key codecs. Secrets stay in Rust and serialization uses zeroizing buffers.
use crate::domain::{AppError, ErrorCode};
use ssh_key::{private::RsaKeypair, rand_core::OsRng, Algorithm, LineEnding, PrivateKey};
use zeroize::Zeroizing;

fn invalid() -> AppError {
    AppError::new(
        ErrorCode::InvalidPrivateKey,
        "Unsupported or invalid OpenSSH private key.",
    )
}

pub struct EncodedKey {
    pub private: Zeroizing<String>,
    pub public: String,
    pub fingerprint: String,
    pub algorithm: String,
}

fn encode(key: PrivateKey, passphrase: Option<&str>) -> Result<EncodedKey, AppError> {
    let algorithm = match key.algorithm() {
        Algorithm::Ed25519 => "ED25519",
        Algorithm::Rsa { .. } => {
            let rsa = key.key_data().rsa().ok_or_else(invalid)?;
            let n = rsa.public.n.as_positive_bytes().ok_or_else(invalid)?;
            let bits = n.len() * 8 - n.first().map_or(0, |b| b.leading_zeros() as usize);
            if bits < 2048 {
                return Err(invalid());
            }
            "RSA"
        }
        _ => return Err(invalid()),
    }
    .to_string();
    // ssh-key 0.6.7's RSA conversion duplicates p instead of using q.
    // Use RustCrypto directly for mathematical validation and the signing proof.
    if let Some(pair) = key.key_data().rsa() {
        use rsa::signature::{Keypair, Signer, Verifier};
        let integer = |n: &ssh_key::Mpint| {
            n.as_positive_bytes()
                .map(rsa::BigUint::from_bytes_be)
                .ok_or_else(invalid)
        };
        let private = rsa::RsaPrivateKey::from_components(
            integer(&pair.public.n)?,
            integer(&pair.public.e)?,
            integer(&pair.private.d)?,
            vec![integer(&pair.private.p)?, integer(&pair.private.q)?],
        )
        .map_err(|_| invalid())?;
        private.validate().map_err(|_| invalid())?;
        let signer = rsa::pkcs1v15::SigningKey::<sha2::Sha512>::new(private);
        let proof = signer
            .try_sign(b"local key consistency check")
            .map_err(|_| invalid())?;
        signer
            .verifying_key()
            .verify(b"local key consistency check", &proof)
            .map_err(|_| invalid())?;
    } else {
        let proof = key
            .sign(
                "remotedock-key-validation",
                ssh_key::HashAlg::Sha256,
                b"local key consistency check",
            )
            .map_err(|_| invalid())?;
        key.public_key()
            .verify(
                "remotedock-key-validation",
                b"local key consistency check",
                &proof,
            )
            .map_err(|_| invalid())?;
    }
    let public = key.public_key().to_openssh().map_err(|_| invalid())?;
    let fingerprint = key
        .public_key()
        .fingerprint(ssh_key::HashAlg::Sha256)
        .to_string();
    let key = match passphrase.filter(|p| !p.is_empty()) {
        Some(secret) => key.encrypt(&mut OsRng, secret).map_err(|_| invalid())?,
        None => key,
    };
    let private = key.to_openssh(LineEnding::LF).map_err(|_| invalid())?;
    // Verify the serialized representation before allowing storage.
    let parsed = PrivateKey::from_openssh(private.as_str()).map_err(|_| invalid())?;
    if parsed.is_encrypted() {
        parsed
            .decrypt(passphrase.ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    }
    Ok(EncodedKey {
        private,
        public,
        fingerprint,
        algorithm,
    })
}

pub fn generate(rsa: bool, passphrase: Option<&str>) -> Result<EncodedKey, AppError> {
    let key = if rsa {
        PrivateKey::from(RsaKeypair::random(&mut OsRng, 4096).map_err(|_| invalid())?)
    } else {
        PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(|_| invalid())?
    };
    encode(key, passphrase)
}

pub fn import(contents: &str, passphrase: Option<&str>) -> Result<EncodedKey, AppError> {
    let key = PrivateKey::from_openssh(contents).map_err(|_| invalid())?;
    let key = if key.is_encrypted() {
        key.decrypt(passphrase.ok_or_else(|| {
            AppError::new(ErrorCode::PassphraseRequired, "Enter the key passphrase.")
        })?)
        .map_err(|_| {
            AppError::new(
                ErrorCode::PassphraseRequired,
                "Unable to unlock the private key.",
            )
        })?
    } else {
        key
    };
    encode(key, passphrase)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ed25519_round_trip_and_identity() {
        let key = generate(false, None).unwrap();
        assert!(key.public.starts_with("ssh-ed25519 "));
        assert!(key.fingerprint.starts_with("SHA256:"));
        let imported = import(&key.private, None).unwrap();
        assert_eq!(key.fingerprint, imported.fingerprint);
        assert_eq!(key.public, imported.public);
        assert!(import("invalid", None).is_err());
    }
    #[test]
    fn encrypted_round_trip() {
        let key = generate(false, Some("test-secret")).unwrap();
        assert!(PrivateKey::from_openssh(key.private.as_str())
            .unwrap()
            .is_encrypted());
        assert!(import(&key.private, None).is_err());
        assert!(import(&key.private, Some("wrong")).is_err());
        assert_eq!(
            import(&key.private, Some("test-secret"))
                .unwrap()
                .fingerprint,
            key.fingerprint
        );
    }
    #[test]
    fn rsa_is_4096_bits() {
        let key = generate(true, None).unwrap();
        let parsed = PrivateKey::from_openssh(key.private.as_str()).unwrap();
        assert_eq!(
            parsed
                .key_data()
                .rsa()
                .unwrap()
                .public
                .n
                .as_positive_bytes()
                .unwrap()
                .len(),
            512
        );
    }
}
