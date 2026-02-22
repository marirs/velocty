use base64::Engine;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};

/// Generate a new 2048-bit RSA keypair for DKIM signing.
/// Returns (private_key_pem, public_key_base64) where the public key is
/// the raw base64 (no PEM headers) suitable for a DNS TXT record.
pub fn generate_keypair() -> Result<(String, String), String> {
    let mut rng = rsa::rand_core::OsRng;
    let private_key =
        RsaPrivateKey::new(&mut rng, 2048).map_err(|e| format!("RSA keygen failed: {}", e))?;

    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| format!("PEM encode failed: {}", e))?;

    let public_der = private_key
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| format!("DER encode failed: {}", e))?;

    let public_b64 = base64::engine::general_purpose::STANDARD.encode(public_der.as_bytes());

    Ok((private_pem.to_string(), public_b64))
}

/// Extract the public key base64 from a stored private key PEM.
pub fn public_key_from_private_pem(private_pem: &str) -> Result<String, String> {
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_pem)
        .map_err(|e| format!("Failed to parse private key: {}", e))?;
    let public_der = private_key
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| format!("DER encode failed: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(public_der.as_bytes()))
}

/// Sign an email message with DKIM.
/// Returns the DKIM-Signature header value to prepend to the message.
pub fn sign_message(
    private_pem: &str,
    selector: &str,
    domain: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<String, String> {
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_pem)
        .map_err(|e| format!("Failed to parse DKIM private key: {}", e))?;

    // Canonicalize body (simple: just ensure trailing CRLF)
    let canon_body = body.replace('\n', "\r\n");
    let canon_body = canon_body.trim_end_matches("\r\n");
    let canon_body = format!("{}\r\n", canon_body);

    // Body hash (SHA-256, base64)
    let body_hash = Sha256::digest(canon_body.as_bytes());
    let bh = base64::engine::general_purpose::STANDARD.encode(body_hash);

    // Build the headers we'll sign (relaxed canonicalization for headers)
    let headers_to_sign = "from:to:subject";
    let canon_headers = format!(
        "from:{}\r\nto:{}\r\nsubject:{}\r\n",
        from.trim().to_lowercase(),
        to.trim().to_lowercase(),
        subject.trim()
    );

    // Build DKIM-Signature header (without b= value yet)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let dkim_header_template = format!(
        "v=1; a=rsa-sha256; c=relaxed/simple; d={}; s={}; t={}; h={}; bh={}; b=",
        domain, selector, timestamp, headers_to_sign, bh
    );

    // The signature input includes the DKIM header itself (with empty b=)
    let sig_input = format!(
        "{}dkim-signature:{}\r\n",
        canon_headers,
        dkim_header_template.trim()
    );

    // Sign with RSA-SHA256
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::{SignatureEncoding, Signer};
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let signature = signing_key
        .try_sign(sig_input.as_bytes())
        .map_err(|e| format!("DKIM signing failed: {}", e))?;

    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    Ok(format!(
        "DKIM-Signature: {}{}",
        dkim_header_template, sig_b64
    ))
}
