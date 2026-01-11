# CantaLoop LSP Strategic Roadmap

## Executive Assessment

> **"The compiler and the LSP are now correctly separated, yet tightly integrated. That's the line most language servers never recover from crossing incorrectly."**

### What We've Nailed ✅

- ✅ **Correct ownership boundaries** (compiler owns truth)
- ✅ **Span-centric design** (unlocks everything)
- ✅ **Snapshot-based querying** (safe, deterministic, testable)
- ✅ **Semantic tokens driven by HIR** (rare, high-quality choice)
- ✅ **Graceful degradation under errors** (critical for UX)
- ✅ **LSP that already understands CantaLoop's execution model**

**Status**: The LSP is no longer "tooling" — it's a **live compiler frontend**.

---

## Critical: Lock Down Before Adding Features

These are structural decisions that are hardest to change later. Address these before Phase 5.

### 1. Freeze the CompilerSnapshot Contract 🔒

**Current State**: `CompilerSnapshot` is the public stability boundary.

**Action Items**:
- [ ] Make `CompilerSnapshot` explicitly immutable
- [ ] Avoid exposing raw internal structures
- [ ] Prefer accessor methods over public fields
- [ ] Document as: *"What any future IDE backend is allowed to know about CantaLoop"*

**Rationale**: This makes snapshot-based testing trivial and provides stability for future IDE backends.

**Implementation Notes**:
```rust
// Current: Fields are pub
pub struct CompilerSnapshot {
    pub csts: HashMap<FileId, CstProgram>,
    pub hir: Option<HirAst>,
    // ...
}

// Target: Immutable with accessors
pub struct CompilerSnapshot {
    csts: HashMap<FileId, CstProgram>,
    hir: Option<HirAst>,
    // ... all fields private
}

impl CompilerSnapshot {
    pub fn cst(&self, file_id: FileId) -> Option<&CstProgram> { ... }
    pub fn hir(&self) -> Option<&HirAst> { ... }
    // ... all access via methods
}
```

### 2. Normalize Span Semantics Across Layers 🔒

**Current State**: Multiple span types in play
- CST spans: `u32` byte offsets (from pest parser)
- HIR spans: `usize` byte offsets (for indexing)
- LSP ranges: line/column (Position/Range)

**Action Items**:
- [ ] Define single canonical internal span type (byte-based)
- [ ] Create explicit adapters at boundaries:
  - CST parser → canonical span
  - Canonical span → HIR
  - Canonical span → LSP range
- [ ] Document span lifecycle and conversion rules

**Rationale**: Prevents subtle bugs when multi-file symbols, generated spans, and desugared constructs enter the picture.

**Implementation Notes**:
```rust
// Proposed: Single canonical span type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalSpan {
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
}

// Explicit adapters
impl From<CstSpan> for CanonicalSpan { ... }
impl From<CanonicalSpan> for LspRange { ... } // via LineIndex
```

### 3. Codify Symbol Stability Rules 🔒

**Current State**: Implicit reliance on symbol stability.

**Action Items**:
- [ ] Define `SymbolStability` enum:
  ```rust
  pub enum SymbolStability {
      UserDefined,        // Functions, variables, modules
      CompilerGenerated,  // Desugared constructs, temporaries
      Unstable,           // May disappear between snapshots
  }
  ```
- [ ] Annotate symbols in semantic index with stability
- [ ] Document which symbols are:
  - User-stable (can be renamed, referenced reliably)
  - Compiler-generated (excluded from rename, maybe hover-only)
  - Unstable (may disappear between snapshots)

**Rationale**: Critical for rename, references, code actions, and future refactors.

**Affected Features**:
- Rename: Only allow renaming stable symbols
- References: Warn or exclude unstable symbols
- Code actions: Don't suggest actions on compiler-generated symbols

---

## High-Impact Next Steps (Ordered by ROI)

### 1. Effect Diagnostics (Phase 5) — Highest Payoff 🎯

**Status**: This is where CantaLoop becomes visibly different.

**What We Already Have**:
- Effect info in HIR
- Execution markers
- Span mappings

**Implementation Plan**:

Start with three canonical diagnostics:

1. **Effectful call in pure context**
   - Diagnostic: "Cannot call effectful function `X` in pure context"
   - Span: Highlight the function call
   - Explanation: "This function requires execution marker (`!`)"

2. **Missing execution marker**
   - Diagnostic: "Effectful function call requires execution marker"
   - Span: Highlight the call site
   - Explanation: "Use `!` to mark execution: `func()!`"

3. **Unhandled effect pipeline**
   - Diagnostic: "Unhandled effect in pipeline"
   - Span: Highlight the pipeline expression
   - Explanation: "Effect pipelines must be executed with `!`"

**Requirements**:
- Each diagnostic must:
  - Point at the exact span
  - Explain why execution is blocked
  - **Be emitted by the compiler, not the LSP**

**Priority**: **CRITICAL** - This alone makes the language feel "alive".

### 2. Code Actions — Compiler-Generated Only ✅

**Principle**: Do not add "smart" LSP-side heuristics.

**Architecture**:
```rust
// In compiler diagnostics:
pub enum Diagnostic {
    Error {
        message: String,
        span: Span,
        fix: Option<Fix>,  // Compiler suggests fix
    },
    // ...
}

pub struct Fix {
    pub span: Span,
    pub replacement: String,
    pub label: String,
}

// In LSP handler:
// Only translates Fix → TextEdit
// No logic, just protocol conversion
```

**Benefits**:
- Safety guarantees (compiler authority)
- Refactorability
- Once this exists, every future feature becomes cheaper

### 3. Cross-File Symbol Resolution (Minimum Viable) 🔄

**Current State**: Single-file only.

**MVP Requirements**:
- [ ] Global symbol table across snapshots
- [ ] Module-level definitions only (start simple)
- [ ] Conservative invalidation (recompile on any change)

**What We DON'T Need Yet**:
- Full module graphs
- Incremental cross-file analysis
- Complex dependency tracking

**Goal**: Even partial cross-file go-to-definition dramatically improves perceived quality.

### 4. Rename (Only After Symbol Stability is Explicit) 🔄

**Current State**: We have 90% of the machinery.

**Remaining 10%**: Policy, not code.

**Decision Points**:
- When is rename allowed? (Only stable symbols)
- What symbols are excluded? (Compiler-generated, unstable)
- What happens on partial failure? (Atomic rename vs. best-effort)

**Order**: Do this **after** effect diagnostics (right order).

---

## Things to Explicitly NOT Do Yet ❌

These are tempting—but premature:

- ❌ Full incremental HIR diffs
- ❌ Background indexing threads
- ❌ Fancy UI visualizations
- ❌ Performance micro-optimizations

**Rationale**: Your architecture already supports these. Let usage pressure justify them.

---

## Testing Strategy That Fits Our Design

We're in a rare position where testing is clean.

### Three-Layer Testing Approach

#### 1. Snapshot Tests (Compiler-Only)
```rust
#[test]
fn test_snapshot_contents() {
    let input = "fn test() -> num { 42 }";
    let snapshot = compile_to_snapshot(input);
    
    // Assert spans, symbols, diagnostics
    assert_eq!(snapshot.hir().unwrap().functions.len(), 1);
    assert!(snapshot.symbols().is_some());
}
```

**Focus**: Input text → snapshot contents

#### 2. Query Tests (compiler_lsp_api)
```rust
#[test]
fn test_position_to_symbol() {
    let snapshot = compile_to_snapshot("let x = 42");
    let pos = Position { line: 0, character: 4 };
    let symbol = snapshot.symbol_at(file_id, pos);
    
    assert_eq!(symbol, Some(SymbolId(1)));
}
```

**Focus**: Position → symbol, Symbol → references, Span → hover info

#### 3. Protocol Tests (Minimal)
```rust
#[test]
fn test_goto_definition_request() {
    let server = setup_server();
    let response = server.handle_goto_definition(params);
    
    assert_matches!(response, Ok(Some(GotoDefinitionResponse::Scalar(_))));
}
```

**Focus**: "Given this request, server responds with X" - No language semantics here.

**Goal**: Keep LSP tests boring—and that's a compliment.

---

## Long-Term: What This Enables

Because we did this right, we've quietly enabled:

### Future Capabilities

1. **Debugger Protocol**
   - Execution-aware debugging
   - Breakpoints on effect boundaries

2. **Execution-Aware Inline Explanations**
   - "This expression executes" vs "This expression is data"
   - Pipeline execution order visualization

3. **Permission-Aware Tooling**
   - Highlight permission-required operations
   - Suggest permission grants

4. **Formalized Refactoring Support**
   - Safe refactors with compiler guarantees
   - Effect-preserving transformations

5. **Non-Editor Tooling**
   - CLI explain tool
   - CI diagnostics
   - Documentation generation

6. **Web-Based IDE**
   - Same LSP backend
   - Different frontend protocol

**Key Insight**: All without rewriting logic.

---

## Bottom Line

> **This is not just "Phase 4 complete".**
>
> **We've built:**
>
> ### **A compiler that happens to speak LSP.**
>
> ### **That's the rare, correct inversion.**

---

## Action Items Summary

### Immediate (Before Phase 5):
1. ✅ Document current achievements
2. [ ] Freeze `CompilerSnapshot` contract (make immutable)
3. [ ] Normalize span semantics (canonical span type)
4. [ ] Codify symbol stability rules (`SymbolStability` enum)

### High Priority (Phase 5):
1. [ ] Effect diagnostics (three canonical diagnostics)
2. [ ] Compiler-generated code actions (Fix struct in diagnostics)
3. [ ] Cross-file symbol resolution (MVP)

### Medium Priority:
1. [ ] Rename (after symbol stability)
2. [ ] Testing infrastructure (three-layer approach)

### Low Priority (Future):
- Incremental compilation optimizations
- Performance micro-optimizations
- Advanced visualizations

---

## Success Metrics

### Quality Indicators:
- ✅ Compiler owns all language logic
- ✅ LSP is pure protocol adapter
- ✅ Span-centric architecture
- ✅ Snapshot-based querying
- ✅ Graceful error handling

### Next Milestones:
- [ ] Effect diagnostics live in editor
- [ ] Code actions from compiler fixes
- [ ] Cross-file navigation works
- [ ] Test suite with three-layer approach

---

**Last Updated**: Based on executive assessment after Phase 4 completion.
