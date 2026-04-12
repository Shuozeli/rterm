# FlatBuffers Dart Codegen — Status

<!-- agent-updated: 2026-04-05T00:30:00Z -->

## What works

### 1. Data types from `flatc --dart`

```bash
flatc --dart --gen-object-api -o mobile/lib/generated crates/rterm-proto/schema/rterm.fbs
```

Generates `mobile/lib/generated/rterm_rterm.protocol_generated.dart` — all FlatBuffers tables, enums, unions. **Requires `--gen-object-api`** to generate `T` classes with `unpack()` and `pack()` methods needed by gRPC serializers.

### 2. gRPC client stubs from `compile_fbs_dart`

```rust
// In crates/rterm-proto/build.rs
grpc_build::compile_fbs_dart(
    &[schema_dir.join("rterm.fbs")],
    &[&schema_dir],
    ".",
).unwrap();
```

Generates `terminalservice_client.dart` with:
- `TerminalServiceClient extends grpc.Client`
- All 10 RPC methods: `session()`, `list_active_sessions()`, `create_session()`, etc.
- Serializers using `request.unpack().pack(builder)` — **no longer throws**
- Deserializers — working
- Valid Dart type names (no `..` prefix) — **FIXED**

**Build script post-processing** (`crates/rterm-proto/build.rs`):
- `compile_fbs_dart` outputs to `OUT_DIR` with wrong import `package:./rterm/protocol_generated.dart`
- Post-processing step replaces this with `import 'rterm_rterm.protocol_generated.dart';` (relative path)
- Both files copied to `mobile/lib/generated/`

---

## Critical Bugs (must fix before use)

### Bug A: Duplicate static field names — all methods named ` _$method_name_snake`

**File:** `grpc-codegen/src/dart_client_gen.rs`

Every RPC generates the same variable name:
```dart
// All 10 RPC methods produce: static final _$method_name_snake = ...
static final _$method_name_snake = grpc.ClientMethod<...>('/rterm.protocol.TerminalService/Session', ...);
static final _$method_name_snake = grpc.ClientMethod<...>('/rterm.protocol.TerminalService/ListActiveSessions', ...); // shadows above!
```

Only the last method descriptor survives — all others are overwritten.

**Fix:** Generate unique names per method: `_$session`, `_$list_active_sessions`, `_$create_session`, etc.

**Status:** Fixed in pure-grpc-rs commit `d5429af`.

---

### Bug B: Wrong import path for generated FlatBuffers types

**File:** Generated `terminalservice_client.dart` line 9:
```dart
import 'package:rterm_rterm.protocol/rterm/protocol_generated.dart'; // package doesn't exist
```

With `proto_path = "rterm_rterm.protocol"`, generates `package:rterm_rterm.protocol/rterm/protocol_generated.dart` — but `rterm_rterm.protocol` is not a Dart package.

**Workaround:** `proto_path = "."` generates `package:./rterm/protocol_generated.dart` (also invalid). Build script post-processing replaces this with a relative import `rterm_rterm.protocol_generated.dart`.

**Status:** Workaround applied in `build.rs`. Bug persists in pure-grpc-rs.

---

### Bug E: Generated type names have `..` prefix (e.g., `..ClientMessage`)

**File:** `grpc-codegen/src/dart_client_gen.rs`

Even after fixing the import path, the generated Dart code contained malformed type references:
```dart
static final _session = grpc.ClientMethod<..ClientMessage, ..ServerMessage>(
//                             ^^^^^^^           ^^^^^^^ — not valid Dart syntax
```

**Root cause:** When `proto_path = "."`, flatbuffers.rs formats `input_type` as `.::TypeName`.
The `extract_type_name()` function did `.replace("::", ".")` which produced `..TypeName`.

**Fix:** `extract_type_name()` now detects and strips the leading `.::` prefix.

**Status:** Fixed in pure-grpc-rs commit `9533eeb`.

---

## Medium Issues

### Issue C: Union `body` returns `dynamic`

**File:** `rterm_rterm.protocol_generated.dart`

```dart
dynamic get body {
  switch (bodyType?.value) {
    case 1: return KeyInput.reader.vTableGetNullable(_bc, _bcOffset, 6);
    // ...
  }
}
```

Manual cast required. Bug in flatc Dart generator.

---

### Issue F: grpc 4.x API mismatch — `runUnary` vs `$createUnaryCall`

**File:** Generated `terminalservice_client.dart`

The generated code calls `runUnary()` inherited from `grpc.Client`:
```dart
final response = await runUnary(_session, request, options: options);
```

But `grpc` package 4.x uses `$createUnaryCall()` instead. The `runUnary` method does not exist in grpc 4.x.

**Fix:** Updated dart_client_gen.rs to generate `$createUnaryCall` for grpc 4.x. Server streaming also fixed to wrap request in `Stream.value()`.

**Status:** Fixed in pure-grpc-rs commit `f933a00`.

---

## Low Issues

### Issue D: 165+ lint warnings in generated code

Unnecessary `!` on non-nullable types and `${x}` instead of `$x`. Bug in flatc Dart generator.

---

## Fix Priority

| Priority | Issue | Fix Location |
|----------|-------|-------------|
| Critical | Bug A: duplicate ` _$method_name_snake` | `grpc-codegen/src/dart_client_gen.rs` — **FIXED** `d5429af` |
| Critical | Bug B: wrong import path | `grpc-build/src/flatbuffers.rs` — workaround in `build.rs` |
| Critical | Bug E: `..ClientMessage` type prefix | `grpc-codegen/src/dart_client_gen.rs` — **FIXED** `9533eeb` |
| ~~Critical~~ | ~~Issue F: `runUnary` vs `$createUnaryCall`~~ | ~~grpc-codegen/src/dart_client_gen.rs`~~ — **FIXED** `df879e0` |
| Critical | Serialization: `unpack()` not generated | **FIXED** — use `flatc --gen-object-api` |
| Medium | Issue C: union `body` returns `dynamic` | flatc Dart generator |
| Low | Issue D: lint warnings | flatc Dart generator |
