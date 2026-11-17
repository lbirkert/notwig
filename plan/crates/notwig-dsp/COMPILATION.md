# Stock DSP Compilation (`notwig-dsp`)

## 1. Role
- Faust preferred for stock DSP definitions; Rust owns wrappers, param/state mapping, graph integration, metadata.
- Compilation produces executable artifacts off-RT.

## 2. Artifacts & Identity
- `DspDef = { typeId, defVersion, sourceHash, compileOpts }`
- `DspArtifact = { artifactId, target, abiVersion, binaryOrIrRef, paramMap, ioMap }`
- Cache key `K = H(typeId, defVersion, sourceHash, compileOpts, toolchainVersion, targetTriple)`

## 3. Workflow (off-RT)
1. Resolve definition for device/type.
2. Cache lookup by `K`; on miss enqueue background compile.
3. Compile via Faust toolchain (libfaust/LLVM backend; pinned/versioned toolchain required).
4. Validate IO/param schema compatibility and run non-RT smoke checks.
5. Instantiate/initialize and integrate via prepared plan swap.

- Lazy compile allowed; deterministic non-blocking fallback/placeholder behavior for RT.

## 4. Caching & Invalidation
- Cache may live in regenerable bundle/cache area; entries versioned by `K` and `abiVersion`.
- Invalidate on changes to source/def/options/toolchain/target affecting codegen.
- Artifact replacement only at safe swap points with compatible state migration.

## 5. Optimization Policy
- Scalar/reference path required for correctness baseline.
- SIMD/vectorized variants optional per config after correctness parity and benchmarks.
- No synchronous compilation in RT path; generated code MUST be RT-safe.
