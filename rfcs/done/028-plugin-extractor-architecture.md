# RFC-028: Plugin Extractor Architecture

**Project:** orbok  
**RFC:** 028  
**Title:** Plugin Extractor Architecture  
**Status:** Implemented (v0.8.0)
**Target Timing:** After built-in extractors stabilize and external extension need is confirmed  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will define whether `orbok` should support plugin-based document extractors.

Plugin extractors could allow custom formats, but they expand the security and maintenance surface. Built-in extractors should stabilize first.

## 2. Motivation

Users may later want support for proprietary formats, domain-specific exports, custom logs, specialized archives, or application-specific document stores.

A plugin architecture could help, but extractor plugins are security-sensitive because they process untrusted local files.

## 3. Activation Conditions

Reconsider this RFC when:

1. Built-in extraction pipeline is stable.
2. Extractor trait is mature.
3. Demand for custom formats is confirmed.
4. Security model is strong enough.
5. Packaging impact is understood.

## 4. Candidate Plugin Models

| Model | Notes |
|---|---|
| no plugins | simplest and safest |
| compile-time feature extractors | controlled, Rust-native |
| external command extractors | flexible, security risk |
| WASM sandbox extractors | safer boundary, complexity |
| dynamic library plugins | powerful, high risk |

Recommended first step, if needed, is compile-time feature extractors or explicit external command extractors with strong warnings.

## 5. Security Requirements

Plugin extractors must address untrusted input, arbitrary code execution, filesystem access, network access, logging of document content, timeouts, memory limits, crash isolation, provenance, and trust.

## 6. Plugin Metadata

Potential manifest:

```toml
name = "custom-extractor"
version = "0.1.0"
supported_extensions = ["foo"]
trust_level = "external-command"
requires_network = false
```

## 7. Non-Goals

This future RFC should not add plugin support before built-in extractors work, allow silent execution of untrusted plugins, bypass source policy, or allow plugins to access arbitrary files by default.

## 8. Expected Decision Output

The activated RFC should produce plugin model, trust boundary, manifest format, installation workflow, execution sandbox, timeout policy, logging policy, user warnings, and test plan.

## 9. Acceptance Criteria

- Security model defined.
- Plugin cannot bypass source allowlist.
- Plugin failures isolated.
- User consent required for external plugins.
- Plugin logging rules defined.
- Packaging impact understood.

## 10. Deferred Decision

Do not implement plugin extractor architecture until built-in extraction coverage and user demand justify it.
