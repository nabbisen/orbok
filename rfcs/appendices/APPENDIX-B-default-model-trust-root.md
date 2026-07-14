# Appendix B — Default Model Trust Root

**Status:** Proposed normative companion to RFC-050

**Evidence captured:** 2026-07-14

**Model:** `intfloat/multilingual-e5-small`

**Provider:** Hugging Face Hub

## 1. Purpose

This appendix supplies the concrete reviewed trust-root data required by
RFC-050. It is source-controlled application metadata, not metadata downloaded
during model installation. Production code must embed an equivalent typed
representation and tests must prove it matches this artifact.

Changing the revision, file set, digest, size, license, identity, or permitted
hosts requires a new security/design review. A moving branch such as `main`
must never appear in a production model URL.

## 2. Normative Manifest

```json
{
  "schema_version": 1,
  "manifest_id": "multilingual-e5-small-hf-614241f6",
  "model": {
    "id": "intfloat/multilingual-e5-small",
    "display_name": "multilingual-e5-small",
    "revision": "614241f622f53c4eeff9890bdc4f31cfecc418b3",
    "role": "embedding",
    "dimension": 384,
    "license": "MIT"
  },
  "transport": {
    "https_only": true,
    "credentials_allowed": false,
    "max_redirects": 1,
    "initial_host": "huggingface.co",
    "permitted_redirect_hosts": ["cas-bridge.xethub.hf.co"],
    "relative_redirects_allowed": false,
    "strip_sensitive_headers_on_redirect": true
  },
  "files": [
    {
      "logical_name": "tokenizer",
      "relative_path": "tokenizer.json",
      "url": "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json",
      "sha256": "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
      "exact_size_bytes": 17082730,
      "max_transfer_bytes": 18000000
    },
    {
      "logical_name": "onnx-model",
      "relative_path": "onnx/model.onnx",
      "url": "https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/onnx/model.onnx",
      "sha256": "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665",
      "exact_size_bytes": 470268510,
      "max_transfer_bytes": 471000000
    }
  ]
}
```

`exact_size_bytes` is part of artifact identity and must match before promotion.
`max_transfer_bytes` is a separate streaming safety limit enforced even when a
server omits or lies about `Content-Length`.

## 3. Redirect and Header Policy

The client starts only at the exact URLs above and implements redirects itself:

1. Accept zero or one redirect.
2. Accept only an absolute `https` URL.
3. The initial request host must be exactly `huggingface.co`.
4. A redirect host must be exactly `cas-bridge.xethub.hf.co`.
5. Reject relative, scheme-relative, downgrade, user-info, alternate-port,
   additional, or differently hosted redirects.
6. Do not configure credentials, cookies, bearer tokens, proxy authorization,
   or document-derived headers for these requests.
7. Before the cross-host request, remove `Authorization`, `Proxy-Authorization`,
   `Cookie`, `Cookie2`, and `Referer` even if a future caller supplied them.
8. Retain only safe transport headers such as `Accept`, `Accept-Encoding`,
   `Range` when deliberately supported, and orbok's non-identifying user agent.
9. Never log redirect query parameters; Xet artifact URLs contain temporary
   signed query values.
10. Construct the production HTTP client with automatic environment/system
    proxy discovery disabled and configure no explicit proxy. Credential-bearing
    `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, and platform proxy settings must
    not affect routing or emit `Proxy-Authorization`. Enterprise proxy support,
    if later required, needs a separate reviewed credential/privacy policy.

Any provider change that requires another host or redirect changes this
reviewed policy before code follows it.

## 4. Evidence and Cross-Checks

Observed on 2026-07-14:

- `git ls-remote https://huggingface.co/intfloat/multilingual-e5-small
  refs/heads/main` resolved the provider repository to full revision
  `614241f622f53c4eeff9890bdc4f31cfecc418b3`.
- The revision-qualified Hugging Face model API with blob metadata reported the
  same repository SHA and the two LFS SHA-256/size pairs recorded above.
- Revision-qualified resolver `HEAD` responses independently repeated the same
  `X-Repo-Commit`, `X-Linked-ETag`, and `X-Linked-Size` values.
- Following each resolver response completed with HTTP 200 after exactly one
  redirect to `cas-bridge.xethub.hf.co`.
- The provider model card at the pinned revision identifies the model as MIT
  licensed, multilingual E5 small, with embedding dimension 384.
- No authentication token or credential was configured or required for these
  observations.

The SHA-256 values are provider LFS object identities cross-checked through two
provider interfaces; they are not hashes generated from bytes accepted by the
orbok downloader. Security review must confirm this evidence before the
manifest is activated. Production installation still hashes every received
temporary file locally and compares it with these source-controlled values.

## 5. Maintenance Rule

This trust root does not follow provider `main`. Upgrading requires a new full
revision, new independently cross-checked metadata, local compatibility and
retrieval tests, security review, and a normal RFC/change-review trail. Existing
verified generations remain usable until the replacement generation is fully
staged and activated under RFC-050.
