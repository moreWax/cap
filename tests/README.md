# Test Organization

This directory contains comprehensive tests for the cap screen capture library, organized by testing level and component.

## Directory Structure

```
tests/
├── common/           # Shared test utilities and helpers
│   └── mod.rs       # Common test functions, mock objects, and assertions
├── unit/            # Unit tests for individual components
│   ├── core/        # Core library components (buffer pool, ring buffer)
│   ├── processing/  # Processing pipeline components
│   ├── capture/     # Capture source implementations
│   ├── config/      # Configuration and validation
│   └── session/     # Session management
├── integration/     # Integration tests combining multiple components
│   ├── session/     # Full session lifecycle tests
│   ├── pipeline/    # End-to-end pipeline tests
│   └── streaming/   # RTSP/file streaming tests
├── e2e/            # End-to-end tests (full application flow)
│   ├── cli/        # CLI interface tests
│   └── gui/        # GUI interface tests
└── fixtures/       # Test data and fixtures
    ├── frames/     # Sample frame data
    ├── configs/    # Sample configuration files
    └── videos/     # Sample video files
```

## Test Categories

### Unit Tests (`tests/unit/`)
- Test individual functions, structs, and modules in isolation
- Use mocks and stubs for dependencies
- Focus on correctness and edge cases
- Fast execution, no external dependencies

### Integration Tests (`tests/integration/`)
- Test interactions between multiple components
- Use real implementations where possible
- Test component boundaries and data flow
- May require external dependencies (GStreamer, etc.)

### End-to-End Tests (`tests/e2e/`)
- Test complete user workflows
- Test CLI and GUI interfaces
- May be slower and require system setup
- Validate real-world usage scenarios

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test --test unit
cargo test --test integration
cargo test --test e2e

# Run tests for specific components
cargo test core::buffer_pool
cargo test processing::pipeline
cargo test session::builder

# Run with output for debugging
cargo test -- --nocapture
```

## Test Naming Conventions

- **Unit tests**: `test_component_functionality`
- **Integration tests**: `test_component_integration`
- **E2E tests**: `test_full_workflow`

## Test Utilities

The `tests/common/` module provides:
- `mock_capture`: Mock capture sources for testing
- `test_frames`: Utilities for creating test frame data
- `assertions`: Custom assertions for frame validation

## Adding New Tests

1. Determine the appropriate test level (unit/integration/e2e)
2. Create the test file in the corresponding directory
3. Use the common utilities where applicable
4. Follow the naming conventions
5. Add comprehensive documentation

## Test Coverage Goals

- **Unit Tests**: 90%+ coverage of individual components
- **Integration Tests**: Cover all major component interactions
- **E2E Tests**: Cover primary user workflows
- **Performance Tests**: Benchmark critical paths
- **Stress Tests**: Test under load and edge conditions