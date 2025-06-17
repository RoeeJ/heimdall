# Git Hooks for Heimdall

This document describes the pre-commit hooks available for the Heimdall project.

## Quick Setup

Run the setup script to configure git hooks:

```bash
./scripts/setup-git-hooks.sh
```

## Available Hooks

### 1. Full Pre-commit Hook (Recommended)

The full pre-commit hook ensures code quality by running comprehensive checks before each commit:

- **Formatting Check**: Ensures code follows Rust formatting standards (`cargo fmt`)
- **Linting**: Runs Clippy with all warnings treated as errors
- **Build Verification**: Builds the project in release mode
- **Test Suite**: Runs the complete test suite

This hook is ideal for final commits before pushing to the repository.

### 2. Fast Pre-commit Hook

The fast pre-commit hook provides quicker feedback during iterative development:

- **Formatting Check**: Ensures code follows Rust formatting standards (`cargo fmt`)
- **Linting**: Runs Clippy with all warnings treated as errors  
- **Compilation Check**: Verifies the code compiles (`cargo check`)
- **No Tests**: Skips the test suite for faster commits

This hook is ideal for frequent local commits during development.

## Manual Hook Management

### Installing Hooks Manually

```bash
# Install full hook
cp .git/hooks/pre-commit.full .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

# Install fast hook
cp .git/hooks/pre-commit.fast .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

### Bypassing Hooks

If you need to commit without running hooks (not recommended):

```bash
git commit --no-verify -m "Your commit message"
```

### Removing Hooks

```bash
rm .git/hooks/pre-commit
```

## Hook Output

The hooks provide colored output to make it easy to see the status of each check:

- 🔍 Running pre-commit checks...
- ✓ Green checkmarks for passed checks
- ✗ Red X marks for failed checks
- Clear error messages explaining what needs to be fixed

## Best Practices

1. **Use the full hook** before pushing to ensure all tests pass
2. **Use the fast hook** during active development for quicker feedback
3. **Always fix issues** identified by the hooks rather than bypassing them
4. **Run `cargo fmt`** before committing to avoid formatting failures
5. **Keep dependencies updated** to ensure clippy checks remain current

## Troubleshooting

### Hook Not Running

Ensure the hook is executable:
```bash
chmod +x .git/hooks/pre-commit
```

### Clippy Warnings

Fix warnings or, if absolutely necessary, add targeted allows:
```rust
#[allow(clippy::specific_lint)]
```

### Test Failures

Run tests locally to debug:
```bash
cargo test --test failing_test_name -- --nocapture
```

### Performance Issues

If the full hook is too slow, switch to the fast hook:
```bash
./scripts/setup-git-hooks.sh
# Select option 2
```