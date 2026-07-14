//! Reviewed trust root and pure transport policy for RFC-050.
//!
//! This module contains no network client and performs no network access. It
//! translates Appendix B into immutable typed data and decisions that a later
//! production client must obey.

use serde::Serialize;
use sha2::Digest as _;
use url::Url;

pub const DEFAULT_MODEL_REVISION: &str = "614241f622f53c4eeff9890bdc4f31cfecc418b3";

const PERMITTED_REDIRECT_HOSTS: &[&str] = &["cas-bridge.xethub.hf.co"];

const DEFAULT_MODEL_FILES: &[TrustedModelFile] = &[
    TrustedModelFile {
        logical_name: "tokenizer",
        relative_path: "tokenizer.json",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json",
        sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
        exact_size_bytes: 17_082_730,
        max_transfer_bytes: 18_000_000,
    },
    TrustedModelFile {
        logical_name: "onnx-model",
        relative_path: "onnx/model.onnx",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx/model.onnx",
        sha256: "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665",
        exact_size_bytes: 470_268_510,
        max_transfer_bytes: 471_000_000,
    },
];

/// Source-controlled representation of Appendix B's normative manifest.
pub const DEFAULT_TRUSTED_MODEL: TrustedModelManifest = TrustedModelManifest {
    schema_version: 1,
    manifest_id: "multilingual-e5-small-hf-614241f6",
    model: TrustedModelIdentity {
        id: "intfloat/multilingual-e5-small",
        display_name: "multilingual-e5-small",
        revision: DEFAULT_MODEL_REVISION,
        role: "embedding",
        dimension: 384,
        license: "MIT",
    },
    transport: TrustedTransportPolicy {
        https_only: true,
        credentials_allowed: false,
        max_redirects: 1,
        initial_host: "huggingface.co",
        permitted_redirect_hosts: PERMITTED_REDIRECT_HOSTS,
        relative_redirects_allowed: false,
        strip_sensitive_headers_on_redirect: true,
    },
    files: DEFAULT_MODEL_FILES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrustedModelManifest {
    pub schema_version: u32,
    pub manifest_id: &'static str,
    pub model: TrustedModelIdentity,
    pub transport: TrustedTransportPolicy,
    pub files: &'static [TrustedModelFile],
}

impl TrustedModelManifest {
    pub fn file_by_path(&self, relative_path: &str) -> Option<&'static TrustedModelFile> {
        self.files
            .iter()
            .find(|file| file.relative_path == relative_path)
    }

    /// Validate invariants rather than trusting even source-controlled data
    /// implicitly. This is useful for parity tests and future reviewed updates.
    pub fn validate(&self) -> Result<(), TrustPolicyError> {
        if self.schema_version != 1 {
            return Err(TrustPolicyError::UnsupportedSchema);
        }
        if self.manifest_id.is_empty()
            || self.model.id.is_empty()
            || self.model.display_name.is_empty()
            || self.model.role.is_empty()
            || self.model.license.is_empty()
            || self.model.dimension == 0
        {
            return Err(TrustPolicyError::InvalidModelIdentity);
        }
        if self.model.revision.len() != 40
            || !self
                .model
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(TrustPolicyError::MovingOrInvalidRevision);
        }
        if self.files.is_empty() {
            return Err(TrustPolicyError::InvalidFileMetadata);
        }
        if !self.transport.https_only
            || self.transport.credentials_allowed
            || self.transport.max_redirects != 1
            || self.transport.initial_host.is_empty()
            || self.transport.permitted_redirect_hosts.is_empty()
            || self.transport.relative_redirects_allowed
            || !self.transport.strip_sensitive_headers_on_redirect
        {
            return Err(TrustPolicyError::InvalidTransportPolicy);
        }

        let mut total_exact_bytes = 0_u64;
        let mut total_transfer_bytes = 0_u64;
        for (index, file) in self.files.iter().enumerate() {
            validate_relative_path(file.relative_path)?;
            validate_sha256(file.sha256)?;
            if file.exact_size_bytes == 0 || file.max_transfer_bytes < file.exact_size_bytes {
                return Err(TrustPolicyError::InvalidFileMetadata);
            }
            total_exact_bytes = total_exact_bytes
                .checked_add(file.exact_size_bytes)
                .ok_or(TrustPolicyError::SizeOverflow)?;
            total_transfer_bytes = total_transfer_bytes
                .checked_add(file.max_transfer_bytes)
                .ok_or(TrustPolicyError::SizeOverflow)?;
            validate_initial_url(self, file, file.url)?;
            if self.files[..index].iter().any(|previous| {
                previous.logical_name == file.logical_name
                    || previous.relative_path == file.relative_path
                    || previous.url == file.url
            }) {
                return Err(TrustPolicyError::DuplicateFileIdentity);
            }
        }
        if total_transfer_bytes < total_exact_bytes {
            return Err(TrustPolicyError::InvalidFileMetadata);
        }
        Ok(())
    }
}

/// Opaque identity of every trust-root field used by readiness and planning.
/// It is crate-private so public callers cannot mint trusted readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrustRootBinding {
    manifest_id: &'static str,
    fingerprint: [u8; 32],
}

pub(crate) fn trust_root_binding(manifest: &'static TrustedModelManifest) -> TrustRootBinding {
    let mut hasher = sha2::Sha256::new();
    hash_u64(&mut hasher, u64::from(manifest.schema_version));
    hash_str(&mut hasher, manifest.manifest_id);
    hash_str(&mut hasher, manifest.model.id);
    hash_str(&mut hasher, manifest.model.display_name);
    hash_str(&mut hasher, manifest.model.revision);
    hash_str(&mut hasher, manifest.model.role);
    hash_u64(&mut hasher, u64::from(manifest.model.dimension));
    hash_str(&mut hasher, manifest.model.license);
    hash_bool(&mut hasher, manifest.transport.https_only);
    hash_bool(&mut hasher, manifest.transport.credentials_allowed);
    hash_u64(&mut hasher, u64::from(manifest.transport.max_redirects));
    hash_str(&mut hasher, manifest.transport.initial_host);
    hash_u64(
        &mut hasher,
        manifest.transport.permitted_redirect_hosts.len() as u64,
    );
    for host in manifest.transport.permitted_redirect_hosts {
        hash_str(&mut hasher, host);
    }
    hash_bool(&mut hasher, manifest.transport.relative_redirects_allowed);
    hash_bool(
        &mut hasher,
        manifest.transport.strip_sensitive_headers_on_redirect,
    );
    hash_u64(&mut hasher, manifest.files.len() as u64);
    for file in manifest.files {
        hash_str(&mut hasher, file.logical_name);
        hash_str(&mut hasher, file.relative_path);
        hash_str(&mut hasher, file.url);
        hash_str(&mut hasher, file.sha256);
        hash_u64(&mut hasher, file.exact_size_bytes);
        hash_u64(&mut hasher, file.max_transfer_bytes);
    }
    let digest = hasher.finalize();
    let mut fingerprint = [0_u8; 32];
    fingerprint.copy_from_slice(&digest);
    TrustRootBinding {
        manifest_id: manifest.manifest_id,
        fingerprint,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrustedModelIdentity {
    pub id: &'static str,
    pub display_name: &'static str,
    pub revision: &'static str,
    pub role: &'static str,
    pub dimension: u32,
    pub license: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrustedTransportPolicy {
    pub https_only: bool,
    pub credentials_allowed: bool,
    pub max_redirects: u8,
    pub initial_host: &'static str,
    pub permitted_redirect_hosts: &'static [&'static str],
    pub relative_redirects_allowed: bool,
    pub strip_sensitive_headers_on_redirect: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TrustedModelFile {
    pub logical_name: &'static str,
    pub relative_path: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub exact_size_bytes: u64,
    pub max_transfer_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpClientPolicy {
    pub automatic_proxy_discovery: bool,
    pub explicit_proxy: bool,
    pub credential_store: bool,
    pub cookie_store: bool,
    pub automatic_referer: bool,
}

/// Required construction settings for the future production HTTP client.
pub const PRODUCTION_HTTP_CLIENT_POLICY: HttpClientPolicy = HttpClientPolicy {
    automatic_proxy_discovery: false,
    explicit_proxy: false,
    credential_store: false,
    cookie_store: false,
    automatic_referer: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderDisposition {
    Retain,
    Strip,
    Reject,
}

/// Decide which headers may cross the reviewed redirect boundary.
pub fn redirect_header_disposition(name: &str) -> HeaderDisposition {
    if [
        "authorization",
        "proxy-authorization",
        "cookie",
        "cookie2",
        "referer",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        return HeaderDisposition::Strip;
    }

    if ["accept", "accept-encoding", "range", "user-agent"]
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        HeaderDisposition::Retain
    } else {
        HeaderDisposition::Reject
    }
}

/// Accept only the exact, revision-qualified source URL in the trust root.
pub fn validate_initial_url(
    manifest: &TrustedModelManifest,
    file: &TrustedModelFile,
    candidate: &str,
) -> Result<Url, TrustPolicyError> {
    if candidate != file.url {
        return Err(TrustPolicyError::UntrustedInitialUrl);
    }
    let parsed = Url::parse(candidate).map_err(|_| TrustPolicyError::InvalidUrl)?;
    validate_common_url(&parsed)?;
    if parsed.host_str() != Some(manifest.transport.initial_host) {
        return Err(TrustPolicyError::UntrustedInitialHost);
    }
    if parsed.query().is_some() {
        return Err(TrustPolicyError::UnexpectedInitialUrlComponent);
    }
    let expected_url = format!(
        "https://{}/{}/resolve/{}/{}",
        manifest.transport.initial_host,
        manifest.model.id,
        manifest.model.revision,
        file.relative_path
    );
    if candidate != expected_url {
        return Err(TrustPolicyError::MovingOrInvalidRevision);
    }
    Ok(parsed)
}

/// Validate one absolute redirect target. `redirect_number` is one-based.
pub fn validate_redirect_url(
    manifest: &TrustedModelManifest,
    location: &str,
    redirect_number: u8,
) -> Result<Url, TrustPolicyError> {
    if redirect_number == 0 || redirect_number > manifest.transport.max_redirects {
        return Err(TrustPolicyError::TooManyRedirects);
    }
    let parsed = Url::parse(location).map_err(|_| TrustPolicyError::RelativeOrInvalidRedirect)?;
    validate_common_url(&parsed)?;
    let host = parsed
        .host_str()
        .ok_or(TrustPolicyError::UntrustedRedirectHost)?;
    if !manifest.transport.permitted_redirect_hosts.contains(&host) {
        return Err(TrustPolicyError::UntrustedRedirectHost);
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicyError {
    UnsupportedSchema,
    InvalidModelIdentity,
    InvalidTransportPolicy,
    MovingOrInvalidRevision,
    InvalidFileMetadata,
    SizeOverflow,
    DuplicateFileIdentity,
    InvalidRelativePath,
    InvalidDigest,
    InvalidUrl,
    UntrustedInitialUrl,
    UntrustedInitialHost,
    RelativeOrInvalidRedirect,
    TooManyRedirects,
    UntrustedRedirectHost,
    InsecureTransport,
    CredentialsForbidden,
    AlternatePortForbidden,
    FragmentForbidden,
    UnexpectedInitialUrlComponent,
}

fn validate_common_url(url: &Url) -> Result<(), TrustPolicyError> {
    if url.scheme() != "https" {
        return Err(TrustPolicyError::InsecureTransport);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(TrustPolicyError::CredentialsForbidden);
    }
    if url.port().is_some() {
        return Err(TrustPolicyError::AlternatePortForbidden);
    }
    if url.fragment().is_some() {
        return Err(TrustPolicyError::FragmentForbidden);
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), TrustPolicyError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(TrustPolicyError::InvalidRelativePath);
    }
    Ok(())
}

fn validate_sha256(digest: &str) -> Result<(), TrustPolicyError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TrustPolicyError::InvalidDigest);
    }
    Ok(())
}

fn hash_str(hasher: &mut sha2::Sha256, value: &str) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value.as_bytes());
}

fn hash_u64(hasher: &mut sha2::Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn hash_bool(hasher: &mut sha2::Sha256, value: bool) {
    hasher.update([u8::from(value)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPENDIX_B: &str =
        include_str!("../../../../rfcs/appendices/APPENDIX-B-default-model-trust-root.md");

    #[test]
    fn typed_manifest_exactly_matches_appendix_b_json() {
        let json = APPENDIX_B
            .split_once("```json\n")
            .and_then(|(_, remainder)| remainder.split_once("\n```").map(|(json, _)| json))
            .expect("Appendix B must contain one JSON code block");
        let appendix: serde_json::Value = serde_json::from_str(json).unwrap();
        let typed = serde_json::to_value(DEFAULT_TRUSTED_MODEL).unwrap();
        assert_eq!(typed, appendix);
        assert_eq!(DEFAULT_TRUSTED_MODEL.validate(), Ok(()));
    }

    #[test]
    fn initial_urls_are_exact_and_revision_qualified() {
        for file in DEFAULT_TRUSTED_MODEL.files {
            assert!(validate_initial_url(&DEFAULT_TRUSTED_MODEL, file, file.url).is_ok());
            assert_eq!(
                validate_initial_url(
                    &DEFAULT_TRUSTED_MODEL,
                    file,
                    &file.url.replace(DEFAULT_MODEL_REVISION, "main")
                ),
                Err(TrustPolicyError::UntrustedInitialUrl)
            );
        }
    }

    #[test]
    fn initial_url_requires_exact_immutable_resolver_structure() {
        let base = DEFAULT_TRUSTED_MODEL.files[0];
        let moving_selector = TrustedModelFile {
            url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json",
            ..base
        };
        assert_eq!(
            validate_initial_url(
                &DEFAULT_TRUSTED_MODEL,
                &moving_selector,
                moving_selector.url
            ),
            Err(TrustPolicyError::MovingOrInvalidRevision)
        );

        let wrong_model = TrustedModelFile {
            url: "https://huggingface.co/another/model/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json",
            ..base
        };
        assert_eq!(
            validate_initial_url(&DEFAULT_TRUSTED_MODEL, &wrong_model, wrong_model.url),
            Err(TrustPolicyError::MovingOrInvalidRevision)
        );

        let wrong_file = TrustedModelFile {
            url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/config.json",
            ..base
        };
        assert_eq!(
            validate_initial_url(&DEFAULT_TRUSTED_MODEL, &wrong_file, wrong_file.url),
            Err(TrustPolicyError::MovingOrInvalidRevision)
        );

        let query = TrustedModelFile {
            url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json?download=1",
            ..base
        };
        assert_eq!(
            validate_initial_url(&DEFAULT_TRUSTED_MODEL, &query, query.url),
            Err(TrustPolicyError::UnexpectedInitialUrlComponent)
        );
    }

    #[test]
    fn manifest_validation_rejects_invalid_path_and_digest() {
        let invalid_path = manifest_with_files(vec![TrustedModelFile {
            relative_path: "../tokenizer.json",
            ..DEFAULT_TRUSTED_MODEL.files[0]
        }]);
        assert_eq!(
            invalid_path.validate(),
            Err(TrustPolicyError::InvalidRelativePath)
        );

        let invalid_digest = manifest_with_files(vec![TrustedModelFile {
            sha256: "not-a-sha256",
            ..DEFAULT_TRUSTED_MODEL.files[0]
        }]);
        assert_eq!(
            invalid_digest.validate(),
            Err(TrustPolicyError::InvalidDigest)
        );
    }

    #[test]
    fn manifest_validation_rejects_duplicates_and_size_overflow() {
        let duplicate = manifest_with_files(vec![
            DEFAULT_TRUSTED_MODEL.files[0],
            DEFAULT_TRUSTED_MODEL.files[0],
        ]);
        assert_eq!(
            duplicate.validate(),
            Err(TrustPolicyError::DuplicateFileIdentity)
        );

        let overflow = manifest_with_files(vec![
            TrustedModelFile {
                exact_size_bytes: u64::MAX,
                max_transfer_bytes: u64::MAX,
                ..DEFAULT_TRUSTED_MODEL.files[0]
            },
            TrustedModelFile {
                exact_size_bytes: u64::MAX,
                max_transfer_bytes: u64::MAX,
                ..DEFAULT_TRUSTED_MODEL.files[1]
            },
        ]);
        assert_eq!(overflow.validate(), Err(TrustPolicyError::SizeOverflow));
    }

    #[test]
    fn redirect_policy_rejects_relative_extra_and_unknown_targets() {
        let accepted = "https://cas-bridge.xethub.hf.co/object?X-Amz-Signature=temporary";
        assert!(validate_redirect_url(&DEFAULT_TRUSTED_MODEL, accepted, 1).is_ok());
        assert_eq!(
            validate_redirect_url(&DEFAULT_TRUSTED_MODEL, "/relative", 1),
            Err(TrustPolicyError::RelativeOrInvalidRedirect)
        );
        assert_eq!(
            validate_redirect_url(&DEFAULT_TRUSTED_MODEL, accepted, 2),
            Err(TrustPolicyError::TooManyRedirects)
        );
        assert_eq!(
            validate_redirect_url(&DEFAULT_TRUSTED_MODEL, "https://example.com/object", 1),
            Err(TrustPolicyError::UntrustedRedirectHost)
        );
    }

    #[test]
    fn redirect_policy_rejects_credentials_downgrade_and_alternate_ports() {
        assert_eq!(
            validate_redirect_url(
                &DEFAULT_TRUSTED_MODEL,
                "https://user:secret@cas-bridge.xethub.hf.co/object",
                1
            ),
            Err(TrustPolicyError::CredentialsForbidden)
        );
        assert_eq!(
            validate_redirect_url(
                &DEFAULT_TRUSTED_MODEL,
                "http://cas-bridge.xethub.hf.co/object",
                1
            ),
            Err(TrustPolicyError::InsecureTransport)
        );
        assert_eq!(
            validate_redirect_url(
                &DEFAULT_TRUSTED_MODEL,
                "https://cas-bridge.xethub.hf.co:8443/object",
                1
            ),
            Err(TrustPolicyError::AlternatePortForbidden)
        );
    }

    #[test]
    fn redirect_headers_are_explicitly_classified() {
        for sensitive in [
            "Authorization",
            "proxy-authorization",
            "COOKIE",
            "Cookie2",
            "Referer",
        ] {
            assert_eq!(
                redirect_header_disposition(sensitive),
                HeaderDisposition::Strip
            );
        }
        for safe in ["Accept", "Accept-Encoding", "Range", "User-Agent"] {
            assert_eq!(redirect_header_disposition(safe), HeaderDisposition::Retain);
        }
        assert_eq!(
            redirect_header_disposition("X-Document-Path"),
            HeaderDisposition::Reject
        );
    }

    #[test]
    fn production_policy_forbids_all_ambient_client_state() {
        assert_eq!(
            PRODUCTION_HTTP_CLIENT_POLICY,
            HttpClientPolicy {
                automatic_proxy_discovery: false,
                explicit_proxy: false,
                credential_store: false,
                cookie_store: false,
                automatic_referer: false,
            }
        );
    }

    fn manifest_with_files(files: Vec<TrustedModelFile>) -> TrustedModelManifest {
        TrustedModelManifest {
            files: Box::leak(files.into_boxed_slice()),
            ..DEFAULT_TRUSTED_MODEL
        }
    }
}
