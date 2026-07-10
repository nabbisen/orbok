# RFC-026: Encrypted Local Indexes

**Project:** orbok  
**RFC:** 026  
**Title:** Encrypted Local Indexes  
**Status:** Withdrawn — key management design requires dedicated security audit before encrypted indexes can be safely implemented. Deferred to post-v1.0.0.
**Target Timing:** After lifecycle, storage, and key-management requirements are clarified  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will decide whether `orbok` should encrypt local indexes, caches, or model-derived data at rest.

Encryption may improve protection against offline access, but it adds key-management complexity and may create a false sense of security if not designed carefully.

## 2. Motivation

`orbok` stores sensitive local derived data: file paths, keyword indexes, embeddings, snippets, extracted segment cache, search history, and localcache payloads.

Users may ask for local-at-rest protection, but encryption raises key-management questions.

## 3. Activation Conditions

Reconsider this RFC when:

1. Storage categories are implemented.
2. localcache integration exists.
3. model/vector storage exists.
4. threat model is reviewed.
5. user need for encrypted indexes is confirmed.

## 4. Threat Models

| Threat | Encryption Help? |
|---|---|
| stolen powered-off laptop without disk encryption | yes |
| another local user account reading app data | maybe |
| malware running as same user | limited |
| app logs leaking content | no, log policy needed |
| cloud sync backup of app data | yes if encrypted before sync |

This RFC must explicitly define which threat is addressed.

## 5. Candidate Scopes

| Scope | Description |
|---|---|
| none | rely on OS disk encryption |
| cache-only | encrypt text-bearing localcache payloads |
| index-only | encrypt keyword/vector indexes |
| full app data | encrypt catalog, indexes, cache |
| profile-based | user selects which data classes are encrypted |

## 6. Key Management Questions

Must define key derivation, passphrase policy, OS keychain integration, unlock flow, key rotation, backup/restore, lost-key behavior, multi-device non-goal, and recovery UX.

Do not enable encryption without key management.

## 7. localcache Encryption

`localcache` supports optional AES-256-GCM payload encryption. This may be useful for text-bearing caches, but enabling it requires a key provider, key storage, failure handling, migration of existing payloads, and UI explanation.

## 8. Non-Goals

This future RFC should not promise protection from malware running as the user, replace OS disk encryption, silently encrypt without recovery UX, or block initial local-first implementation.

## 9. Expected Decision Output

The activated RFC should produce encryption scope, threat model, key management design, storage impact, performance impact, migration plan, unlock UX, and recovery behavior.

## 10. Acceptance Criteria

- Threat model explicitly defined.
- Key management designed.
- User understands lost-key implications.
- Performance measured.
- Migration path exists.
- Cleanup/repair tools handle encrypted data.
- localcache encryption use documented if enabled.

## 11. Deferred Decision

Do not enable encrypted local indexes by default until key management is designed.
