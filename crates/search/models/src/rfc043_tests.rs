//! RFC-043 test router.
use crate::trust::{
    TrustedModelFile, TrustedModelIdentity, TrustedModelManifest, TrustedTransportPolicy,
};

const TEST_FILES: &[TrustedModelFile] = &[
    TrustedModelFile {
        logical_name: "tokenizer",
        relative_path: "tokenizer.json",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json",
        sha256: "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
        exact_size_bytes: 2,
        max_transfer_bytes: 2,
    },
    TrustedModelFile {
        logical_name: "onnx-model",
        relative_path: "onnx/model.onnx",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx/model.onnx",
        sha256: "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        exact_size_bytes: 1,
        max_transfer_bytes: 1,
    },
];

const TEST_REDIRECT_HOSTS: &[&str] = &["cas-bridge.xethub.hf.co"];

const TEST_MANIFEST: TrustedModelManifest = TrustedModelManifest {
    schema_version: 1,
    manifest_id: "test-manifest",
    model: TrustedModelIdentity {
        id: "intfloat/multilingual-e5-small",
        display_name: "multilingual-e5-small",
        revision: "614241f622f53c4eeff9890bdc4f31cfecc418b3",
        role: "embedding",
        dimension: 384,
        license: "MIT",
    },
    transport: TrustedTransportPolicy {
        https_only: true,
        credentials_allowed: false,
        max_redirects: 1,
        initial_host: "huggingface.co",
        permitted_redirect_hosts: TEST_REDIRECT_HOSTS,
        relative_redirects_allowed: false,
        strip_sensitive_headers_on_redirect: true,
    },
    files: TEST_FILES,
};

mod rfc043_download_plan;
mod rfc043_readiness;
