# Upstream Reference Audit

**Retrieval date:** 2026-07-12  
**Purpose:** Fix the exact upstream revisions consulted while designing the Windows-first local LLM privacy gateway.  
**Product dependency status:** None of these repositories is a runtime or build dependency.

## Rules for using these references

- Checkouts live under ignored `references/` and are not distributed.
- The implementation is independent by default.
- A concept or public protocol behavior may be reimplemented; copied source requires an explicit license review and file-level attribution.
- No source is copied from a repository without a complete license grant.
- Upstream benchmark claims are not accepted as product claims without reproduction on this project's fixtures.

## Pinned repositories

### jayluxferro/llm-redactor

- URL: https://github.com/jayluxferro/llm-redactor
- Commit: `8a7f3a1e832bec6d1551277939c94ae96f3876d6`
- Commit date: `2026-05-30T17:29:39+00:00`
- Declared license: `MIT` in `README.md` and `pyproject.toml`
- Root license file: absent at the pinned revision
- README SHA-256: `7a746bcc247f7c2f86646cade9c480ae50e1a381b4d7adcd5dba9938ce2ece2f`
- `pyproject.toml` SHA-256: `0be13313a795dc187a25909efa367068b8b9f471628a97a42c59bfdc3fe249ee`
- Legal treatment: research only; do not copy code until the repository contains or otherwise supplies a complete license grant.

Relevant observations:

- It is the closest functional reference: FastAPI proxying, regex/NER detection, placeholders, OpenAI Chat Completions, Anthropic Messages, SSE, MCP, and a synthetic leak benchmark.
- `src/llm_redactor/transport/http_proxy.py` either refuses or transparently bypasses OpenAI requests containing tools/functions. The target product must inspect supported tool strings rather than silently bypassing them.
- The same proxy detects Anthropic signed thinking blocks and forwards such requests as raw bytes, bypassing content transformation. The target product instead preserves signed blocks as raw JSON values while inspecting separately approved fields.
- Its streaming restoration accumulates redacted output but emits incomplete placeholder prefixes before the token is complete. A two-chunk reproduction produced `Contact ⟨EMAalice@example.com now` instead of `Contact alice@example.com now`. The target implementation must retain an un-emitted bounded tail and test every placeholder split point.
- Its sequential placeholders are request/session scoped. The target uses profile-isolated HMAC tokens and an encrypted persistent reverse map to support client-resubmitted history.
- The published leak benchmark is useful as a methodology reference, but its English-focused results cannot establish Chinese detector quality.

Accepted ideas:

- distinct detection, transformation, transport, and evaluation modules;
- exact-match restoration as the safe default;
- synthetic leak fixtures and utility evaluation;
- explicit refusal modes.

Rejected ideas:

- tool-request bypass;
- recursive or schema-blind mutation;
- emitting partial placeholder prefixes;
- treating README benchmark numbers as production guarantees.

### openziti/llm-gateway

- URL: https://github.com/openziti/llm-gateway
- Commit: `026fea665c5daa34f7687f7af26341cfcd05df6a`
- Commit date: `2026-06-25T15:14:56+00:00`
- License: Apache-2.0
- License SHA-256: `7986f7c237c744491414962f37d990e7c440a50bbc6b59573364642bcbd92b28`

Relevant observations:

- The project demonstrates a small single-binary gateway with provider boundaries, SSE utilities, virtual keys, Windows releases, and a mock model.
- `gateway/apikeys.go` stores configured local keys in plaintext. The target product stores only protected key material and never writes local or upstream keys to ordinary configuration.
- `providers/streaming.go` is a useful example of immediate SSE flush behavior, but it serializes its own event model rather than preserving arbitrary provider events.
- Routing, load balancing, semantic routing, OpenZiti, zrok, metrics export, and public gateway exposure are outside the MVP.

Accepted ideas:

- single-binary lifecycle;
- provider interface boundaries;
- virtual-key middleware;
- explicit SSE writer tests;
- mock-upstream integration tests.

Rejected ideas:

- public listener support;
- plaintext key configuration;
- provider translation and model routing;
- external metrics export.

### data-privacy-stack/presidio

- URL: https://github.com/data-privacy-stack/presidio
- Commit: `16ab0fd8818f76c98de487534e96f98689531e84`
- Commit date: `2026-07-07T15:56:06+03:00`
- License: MIT
- License SHA-256: `f3e86ee59a49bcfb0d9a9547484d55224ea7b2d04f95b1947b4d18d17f6de535`

Relevant observations:

- Presidio provides mature recognizer interfaces, typed detection results, confidence scores, context enhancement, checksum-oriented recognizers, and extensive tests.
- `recognizer_result.py` models span intersection, containment, equality, and conflict. The target adopts explicit typed spans but uses deterministic product priorities before confidence.
- Its Python/NLP runtime and container-oriented deployment are too large for the Windows MVP; the shallow checkout occupies roughly 230 MB even without model weights.
- Its existing recognizers and scores are not assumed to cover Chinese identifiers or product-specific secrets correctly.

Accepted ideas:

- pluggable detector interface;
- explanation and source metadata on detections;
- checksum validation;
- explicit overlap-resolution tests;
- extensive false-positive fixtures.

Rejected ideas:

- embedding the Python service;
- requiring spaCy or a model download in the MVP;
- using confidence alone to resolve security-critical overlaps.

### sherwinvishesh/SurrogateShield

- URL: https://github.com/sherwinvishesh/SurrogateShield
- Commit: `3175b88e11ff9500fbe4de8e8aee61d9237aec35`
- Commit date: `2026-07-07T18:05:32-07:00`
- License: Apache-2.0
- License SHA-256: `75523b72ee2fe1dc68232d70e08d2cf4b3a22e70b1dc6b7be9ce809fb8766a81`

Relevant observations:

- The project separates pattern detection, entity tracing, contextual checks, surrogate generation, encrypted mapping, reconstruction, and evaluation.
- `shadow_map.py` encrypts the map with AES-GCM but stores the raw session key next to the encrypted map. Filesystem permissions are the primary remaining protection. The target stores master material in Windows Credential Manager and authenticates map metadata as associated data.
- Its realistic surrogate generation is useful for later utility experiments but is excluded from the MVP because collision and false-context behavior need independent evaluation.
- Its Torch, Transformers, and spaCy dependencies are unsuitable for the base installer.

Accepted ideas:

- separate map abstraction;
- authenticated encryption;
- utility evaluation of placeholder versus realistic surrogate approaches;
- per-session deletion semantics as a later policy option.

Rejected ideas:

- an adjacent plaintext key file;
- mandatory ML dependencies;
- realistic surrogate values in the MVP.

### openai/privacy-filter

- URL: https://github.com/openai/privacy-filter
- Commit: `f7f00ca7fb869683eb732c010299d901457f19c3`
- Commit date: `2026-04-22T12:55:02-07:00`
- License: Apache-2.0
- License SHA-256: `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`

Relevant observations:

- The repository offers a local token-classification API, constrained span decoding, evaluation modes, and fine-tuning examples.
- The model has a fixed eight-category taxonomy and is primarily English. Its own documentation warns of non-English degradation, domain shift, over-redaction, and missed novel secrets.
- The model implementation and weights are not part of the base MVP. A later optional model adapter must be benchmarked against the same detector interface and Chinese synthetic corpus.

Accepted ideas:

- model detectors behind the same typed-span interface;
- operating-point calibration;
- BIOES span coherence tests;
- explicit in-domain evaluation before enabling a model.

Rejected ideas:

- mandatory model downloads;
- representing the model as complete anonymization;
- assuming English metrics transfer to Chinese.

## Cross-project conclusions

1. No inspected project provides the complete target combination of Windows desktop UX, native OpenAI and Anthropic profiles, Chinese deterministic identifiers, secure Windows key storage, protocol-aware tool filtering, persistent HMAC placeholders, and metadata-only activity records.
2. Blind recursive JSON scanning is incompatible with signed or opaque protocol fields.
3. Streaming restoration must withhold partial placeholder prefixes; accumulating and re-diffing after already emitting the prefix is incorrect.
4. Encryption at rest is insufficient if the data-encryption key is stored adjacent to the ciphertext.
5. Deterministic rules and user dictionaries are the only defensible base-MVP detectors; local ML remains an optional, separately evaluated extension.
6. Upstream compatibility must be expressed as a pinned SDK/fixture matrix, not a universal compatibility claim.

## Code provenance statement

At the time of this audit, no source code from any reference repository has been copied into the product repository.
