//! Skia rendering (spec §7.1, §7.2, ADR 0003): render a Document region at screen resolution,
//! Flatten at full resolution, the highlight/obfuscate/shadow effects, resampling.
//!
//! Skia is initialised lazily, on the first Annotation, never before the first paint (§1.3).
