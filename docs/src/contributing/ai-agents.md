# Working with AI Agents

As AI coding agents continue to improve, generated code is increasingly a part
of the software development process, and indeed projects that plan to use
AI-generated code should be structured to provide maximum context--including
goals, style and API design guidelines, and non-negotiable development rules--to
make agents effective. The `checkle` codebase is designed to do just that.
Starting with AGENTS.md and including additional context in the `context/`
directory, the codebase comes with many specific requirements and guidelines
that all AI agents must follow to ensure code quality and maintainability.

## Strict Rule Compliance

AI agents working on checkle must follow all development rules without
exception. At minimum, this includes:

- **Quality Checks**: Run `cargo fmt`, `cargo check`, and
  `cargo clippy --all-targets --all-features -- -D warnings` before declaring
  any work complete
- **No Lint Suppressions**: Never add `#[allow()]` lint suppressions without
  explicit permission from the project maintainer. `clippy` lints operate under
  an opt-out default principle, which means project lints are exceptionally
  strict and demanding unless exceptions have been explicitly approved. This
  keeps coding agents "on the rails" quite effectively while also providing
  specific, achievable feedback on how to verify that a feature is actually
  finished.
- **Three-Test Rule**: Every change must include at least 3 new, improved tests,
  or replaced tests

## Frequent Context Loading

Before making any changes, AI agents **must** read and understand these
documents:

1. **[AGENTS.md](https://github.com/nrminor/checkle/blob/main/AGENTS.md)** -
   Complete development guidelines and project rules
2. **[README.md](https://github.com/nrminor/checkle/blob/main/README.md)** -
   Project overview and goals
3. **[TIGER_STYLE.md](https://raw.githubusercontent.com/tigerbeetle/tigerbeetle/refs/heads/main/docs/TIGER_STYLE.md)** -
   World-class software robustness principles
4. **[GRUG BRAIN DEVELOPER](https://grugbrain.dev/)** - Pragmatic simplicity
   principles

It is also recommended that agents review these documents _after_ implementing a
feature to ensure that the submitted code is standards-compliant and leaves the
codebase better than it was found.

## Essential Guidelines for AI Agents

### Performance Focus

checkle is designed for bioinformatics workflows with terabyte-scale files.
Always consider:

- Multicore utilization and parallel processing
- Memory efficiency for large file handling
- Merkle tree optimization opportunities
- Buffer reuse and minimal allocations

### Code Quality Standards

The project enforces extremely strict quality standards:

- Zero clippy warnings allowed
- Comprehensive test coverage required
- No `unwrap()` calls in production code
- Proper error handling with context

### Balance Principles

Follow both development philosophies:

- **Tiger Style**: Robustness, assertions, resource limits
- **Grug Brain**: Simplicity, avoiding premature complexity

When these conflict, prefer solutions that are both robust AND simple.

### Bioinformatics Context

Remember that checkle serves genomics researchers who:

- Work with files that can be 500GB+ each
- Need reliable integrity verification for critical data
- Require fast batch processing of many large files
- Value performance and correctness over feature richness

## Critical Requirements

### Documentation Reading

Working without reading the required documents (AGENTS.md, README.md,
TIGER_STYLE.md, GRUG BRAIN DEVELOPER) is **unacceptable** and will result in
code rejection.

### Error Prevention

Common mistakes AI agents must avoid:

- Adding dependencies without approval
- Skipping quality checks
- Writing tests that only verify assertions
- Adding complexity without clear benefit
- Ignoring performance implications

### Success Criteria

Code is considered complete only when:

1. All quality checks pass without warnings
2. At least 3 meaningful tests are included
3. Performance implications have been considered
4. Tiger Style and Grug Brain principles are balanced
5. The change serves the bioinformatics use case

## Getting Started

1. Read all required documentation
2. Understand the specific task requirements
3. Consider performance and simplicity implications
4. Implement with comprehensive error handling
5. Add thorough tests
6. Run all quality checks
7. Document any architectural decisions

By following these guidelines, AI agents can contribute effectively to checkle
while maintaining the high standards that make it reliable for critical
bioinformatics workflows.
