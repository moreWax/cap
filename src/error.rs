//! # Comprehensive Error Handling System
//!
//! This module provides a sophisticated error handling system for the screen capture library,
//! featuring hierarchical error types, error classification traits, and comprehensive error context.
//!
//! ## Architecture
//!
//! The error system is built around several key components:
//!
//! - **Error Types**: Hierarchical error types with rich context and metadata
//! - **Error Traits**: Classification and recovery traits for different error handling patterns
//! - **Error Context**: Rich metadata including timestamps, operation context, and recovery suggestions
//! - **Error Chaining**: Proper error source tracking and propagation
//!
//! ## Error Classification
//!
//! Errors are classified using traits:
//!
//! - `Retryable`: Errors that can be retried
//! - `Recoverable`: Errors that can be recovered from with fallback strategies
//! - `Fatal`: Errors that cannot be recovered from
//! - `Transient`: Temporary errors that may resolve themselves
//!
//! ## Usage
//!
//! ```rust
//! use hybrid_screen_capture::error::{CaptureError, ErrorContext, Retryable};
//!
//! // Create an error with context
//! let error = CaptureError::processing("frame_resize", "invalid dimensions")
//!     .with_context("resizing frame to 1920x1080")
//!     .with_recovery_suggestion("Check input frame dimensions before resizing");
//!
//! // Check if error is retryable
//! if error.is_retryable() {
//!     // Implement retry logic
//! }
//! ```

use std::{error::Error as StdError, fmt, time::SystemTime};

/// Severity levels for errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Debug-level errors that don't affect operation
    Debug,
    /// Informational errors
    Info,
    /// Warnings that may indicate potential issues
    Warning,
    /// Errors that affect operation but can be recovered from
    Error,
    /// Critical errors that require immediate attention
    Critical,
    /// Fatal errors that cannot be recovered from
    Fatal,
}

/// Core error context containing metadata about when and where an error occurred
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// When the error occurred
    pub timestamp: SystemTime,
    /// The operation being performed when the error occurred
    pub operation: Option<String>,
    /// Additional context about the error
    pub context: Option<String>,
    /// Suggested recovery action
    pub recovery_suggestion: Option<String>,
    /// Source location information
    pub source_location: Option<String>,
    /// Error severity level
    pub severity: ErrorSeverity,
    /// Whether this error is retryable
    pub retryable: bool,
    /// Whether this error is recoverable
    pub recoverable: bool,
    /// Additional metadata as key-value pairs
    pub metadata: std::collections::HashMap<String, String>,
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now(),
            operation: None,
            context: None,
            recovery_suggestion: None,
            source_location: None,
            severity: ErrorSeverity::Error,
            retryable: false,
            recoverable: false,
            metadata: std::collections::HashMap::new(),
        }
    }
}

impl ErrorContext {
    /// Create a new error context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the operation that was being performed
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    /// Add additional context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Set recovery suggestion
    pub fn with_recovery_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.recovery_suggestion = Some(suggestion.into());
        self
    }

    /// Set source location
    pub fn with_source_location(mut self, location: impl Into<String>) -> Self {
        self.source_location = Some(location.into());
        self
    }

    /// Set severity level
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Mark as retryable
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    /// Mark as recoverable
    pub fn recoverable(mut self) -> Self {
        self.recoverable = true;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Base error type for the screen capture library
#[derive(Debug)]
pub enum CaptureError {
    /// Configuration validation errors
    Config {
        field: String,
        value: String,
        reason: String,
        context: ErrorContext,
    },
    /// Screen capture initialization failures
    CaptureInit {
        platform: String,
        reason: String,
        context: ErrorContext,
    },
    /// Frame capture failures
    FrameCapture {
        reason: String,
        context: ErrorContext,
    },
    /// Processing pipeline errors
    Processing {
        operation: String,
        reason: String,
        context: ErrorContext,
    },
    /// Streaming and output errors
    Streaming {
        target: String,
        reason: String,
        context: ErrorContext,
    },
    /// Resource allocation failures
    Resource {
        resource: String,
        reason: String,
        context: ErrorContext,
    },
    /// Platform-specific errors
    Platform {
        platform: String,
        code: Option<i32>,
        reason: String,
        context: ErrorContext,
    },
    /// I/O errors
    Io {
        operation: String,
        path: Option<String>,
        source: std::io::Error,
        context: ErrorContext,
    },
    /// GStreamer errors
    /// Note: store textual details to avoid unconditional dependency on the
    /// `gstreamer` crate. Detailed conversion from `gstreamer` errors is
    /// provided behind the `gstreamer` feature so consumers may include the
    /// dependency when desired.
    GStreamer {
        element: Option<String>,
        message: String,
        context: ErrorContext,
    },
    /// External library errors
    External {
        library: String,
        source: Box<dyn StdError + Send + Sync>,
        context: ErrorContext,
    },
    /// Timeout errors
    Timeout {
        operation: String,
        duration_ms: u64,
        context: ErrorContext,
    },
    /// Validation errors
    Validation {
        field: String,
        constraint: String,
        value: String,
        context: ErrorContext,
    },
    /// State errors (invalid state transitions)
    State {
        current_state: String,
        attempted_operation: String,
        reason: String,
        context: ErrorContext,
    },
    /// Network errors
    Network {
        operation: String,
        address: Option<String>,
        source: Option<Box<dyn StdError + Send + Sync>>,
        context: ErrorContext,
    },
    /// Authentication/authorization errors
    Auth {
        operation: String,
        reason: String,
        context: ErrorContext,
    },
    /// Custom errors with arbitrary data
    Custom {
        category: String,
        message: String,
        data: serde_json::Value,
        context: ErrorContext,
    },
}

impl CaptureError {
    /// Create a configuration error
    pub fn config(
        field: impl Into<String>,
        value: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Config {
            field: field.into(),
            value: value.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a capture initialization error
    pub fn capture_init(platform: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::CaptureInit {
            platform: platform.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a frame capture error
    pub fn frame_capture(reason: impl Into<String>) -> Self {
        Self::FrameCapture {
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a processing error
    pub fn processing(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Processing {
            operation: operation.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a streaming error
    pub fn streaming(target: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Streaming {
            target: target.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a resource error
    pub fn resource(resource: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Resource {
            resource: resource.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a platform error
    pub fn platform(
        platform: impl Into<String>,
        code: Option<i32>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Platform {
            platform: platform.into(),
            code,
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create an I/O error
    pub fn io(operation: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            operation: operation.into(),
            path: None,
            source,
            context: ErrorContext::new(),
        }
    }

    /// Create a GStreamer error
    pub fn gstreamer(element: Option<String>, message: impl Into<String>) -> Self {
        Self::GStreamer {
            element,
            message: message.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create an external library error
    pub fn external(
        library: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self::External {
            library: library.into(),
            source: Box::new(source),
            context: ErrorContext::new(),
        }
    }

    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>, duration_ms: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration_ms,
            context: ErrorContext::new(),
        }
    }

    /// Create a validation error
    pub fn validation(
        field: impl Into<String>,
        constraint: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self::Validation {
            field: field.into(),
            constraint: constraint.into(),
            value: value.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a state error
    pub fn state(
        current_state: impl Into<String>,
        attempted_operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::State {
            current_state: current_state.into(),
            attempted_operation: attempted_operation.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a network error
    pub fn network(operation: impl Into<String>) -> Self {
        Self::Network {
            operation: operation.into(),
            address: None,
            source: None,
            context: ErrorContext::new(),
        }
    }

    /// Create an authentication error
    pub fn auth(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Auth {
            operation: operation.into(),
            reason: reason.into(),
            context: ErrorContext::new(),
        }
    }

    /// Create a custom error
    pub fn custom(
        category: impl Into<String>,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self::Custom {
            category: category.into(),
            message: message.into(),
            data,
            context: ErrorContext::new(),
        }
    }

    /// Add context to the error
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context_mut().context = Some(context.into());
        self
    }

    /// Add operation context
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.context_mut().operation = Some(operation.into());
        self
    }

    /// Add recovery suggestion
    pub fn with_recovery_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.context_mut().recovery_suggestion = Some(suggestion.into());
        self
    }

    /// Set severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.context_mut().severity = severity;
        self
    }

    /// Mark as retryable
    pub fn retryable(mut self) -> Self {
        self.context_mut().retryable = true;
        self
    }

    /// Mark as recoverable
    pub fn recoverable(mut self) -> Self {
        self.context_mut().recoverable = true;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context_mut().metadata.insert(key.into(), value.into());
        self
    }

    /// Get the error context
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::Config { context, .. } => context,
            Self::CaptureInit { context, .. } => context,
            Self::FrameCapture { context, .. } => context,
            Self::Processing { context, .. } => context,
            Self::Streaming { context, .. } => context,
            Self::Resource { context, .. } => context,
            Self::Platform { context, .. } => context,
            Self::Io { context, .. } => context,
            Self::GStreamer { context, .. } => context,
            Self::External { context, .. } => context,
            Self::Timeout { context, .. } => context,
            Self::Validation { context, .. } => context,
            Self::State { context, .. } => context,
            Self::Network { context, .. } => context,
            Self::Auth { context, .. } => context,
            Self::Custom { context, .. } => context,
        }
    }

    /// Get mutable reference to error context
    fn context_mut(&mut self) -> &mut ErrorContext {
        match self {
            Self::Config { context, .. } => context,
            Self::CaptureInit { context, .. } => context,
            Self::FrameCapture { context, .. } => context,
            Self::Processing { context, .. } => context,
            Self::Streaming { context, .. } => context,
            Self::Resource { context, .. } => context,
            Self::Platform { context, .. } => context,
            Self::Io { context, .. } => context,
            Self::GStreamer { context, .. } => context,
            Self::External { context, .. } => context,
            Self::Timeout { context, .. } => context,
            Self::Validation { context, .. } => context,
            Self::State { context, .. } => context,
            Self::Network { context, .. } => context,
            Self::Auth { context, .. } => context,
            Self::Custom { context, .. } => context,
        }
    }

    /// Get the error category as a string
    pub fn category(&self) -> &'static str {
        match self {
            Self::Config { .. } => "config",
            Self::CaptureInit { .. } => "capture_init",
            Self::FrameCapture { .. } => "frame_capture",
            Self::Processing { .. } => "processing",
            Self::Streaming { .. } => "streaming",
            Self::Resource { .. } => "resource",
            Self::Platform { .. } => "platform",
            Self::Io { .. } => "io",
            Self::GStreamer { .. } => "gstreamer",
            Self::External { .. } => "external",
            Self::Timeout { .. } => "timeout",
            Self::Validation { .. } => "validation",
            Self::State { .. } => "state",
            Self::Network { .. } => "network",
            Self::Auth { .. } => "auth",
            Self::Custom { .. } => "custom",
        }
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Config {
                field,
                value,
                reason,
                ..
            } => {
                write!(
                    f,
                    "Configuration error in '{}': {} (value: {})",
                    field, reason, value
                )
            }
            CaptureError::CaptureInit {
                platform, reason, ..
            } => {
                write!(
                    f,
                    "Failed to initialize capture on {}: {}",
                    platform, reason
                )
            }
            CaptureError::FrameCapture { reason, .. } => {
                write!(f, "Frame capture failed: {}", reason)
            }
            CaptureError::Processing {
                operation, reason, ..
            } => {
                write!(f, "Processing failed during {}: {}", operation, reason)
            }
            CaptureError::Streaming { target, reason, .. } => {
                write!(f, "Streaming to {} failed: {}", target, reason)
            }
            CaptureError::Resource {
                resource, reason, ..
            } => {
                write!(f, "Resource allocation failed for {}: {}", resource, reason)
            }
            CaptureError::Platform {
                platform,
                code,
                reason,
                ..
            } => {
                if let Some(code) = code {
                    write!(
                        f,
                        "Platform error on {} (code {}): {}",
                        platform, code, reason
                    )
                } else {
                    write!(f, "Platform error on {}: {}", platform, reason)
                }
            }
            CaptureError::Io {
                operation,
                path,
                source,
                ..
            } => {
                if let Some(path) = path {
                    write!(
                        f,
                        "I/O error during {} on '{}': {}",
                        operation, path, source
                    )
                } else {
                    write!(f, "I/O error during {}: {}", operation, source)
                }
            }
            CaptureError::GStreamer {
                element, message, ..
            } => {
                if let Some(element) = element {
                    write!(f, "GStreamer error in element '{}': {}", element, message)
                } else {
                    write!(f, "GStreamer error: {}", message)
                }
            }
            CaptureError::External {
                library, source, ..
            } => {
                write!(f, "External library error in {}: {}", library, source)
            }
            CaptureError::Timeout {
                operation,
                duration_ms,
                ..
            } => {
                write!(f, "Timeout during {} after {}ms", operation, duration_ms)
            }
            CaptureError::Validation {
                field,
                constraint,
                value,
                ..
            } => {
                write!(
                    f,
                    "Validation failed for '{}': {} (value: {})",
                    field, constraint, value
                )
            }
            CaptureError::State {
                current_state,
                attempted_operation,
                reason,
                ..
            } => {
                write!(
                    f,
                    "Invalid state transition from '{}' when attempting '{}': {}",
                    current_state, attempted_operation, reason
                )
            }
            CaptureError::Network {
                operation, address, ..
            } => {
                if let Some(address) = address {
                    write!(f, "Network error during {} on {}", operation, address)
                } else {
                    write!(f, "Network error during {}", operation)
                }
            }
            CaptureError::Auth {
                operation, reason, ..
            } => {
                write!(f, "Authentication error during {}: {}", operation, reason)
            }
            CaptureError::Custom {
                category, message, ..
            } => {
                write!(f, "Custom error [{}]: {}", category, message)
            }
        }
    }
}

impl StdError for CaptureError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::External { source, .. } => Some(source.as_ref()),
            Self::Network {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Result type alias using our custom error type
pub type CaptureResult<T> = Result<T, CaptureError>;

/// Trait for errors that can be retried
pub trait Retryable {
    /// Check if this error can be retried
    fn is_retryable(&self) -> bool;

    /// Get the recommended retry delay in milliseconds
    fn retry_delay_ms(&self) -> Option<u64> {
        None
    }

    /// Get the maximum number of retry attempts
    fn max_retries(&self) -> Option<usize> {
        None
    }
}

impl Retryable for CaptureError {
    fn is_retryable(&self) -> bool {
        self.context().retryable
            || matches!(
                self,
                Self::Timeout { .. }
                    | Self::Network { .. }
                    | Self::Resource { .. }
                    | Self::Io { .. }
            )
    }

    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            Self::Timeout { .. } => Some(1000), // 1 second
            Self::Network { .. } => Some(2000), // 2 seconds
            Self::Resource { .. } => Some(500), // 500ms
            Self::Io { .. } => Some(100),       // 100ms
            _ => None,
        }
    }

    fn max_retries(&self) -> Option<usize> {
        match self {
            Self::Timeout { .. } => Some(3),
            Self::Network { .. } => Some(5),
            Self::Resource { .. } => Some(10),
            Self::Io { .. } => Some(3),
            _ => None,
        }
    }
}

/// Trait for errors that can be recovered from
pub trait Recoverable {
    /// Check if this error can be recovered from
    fn is_recoverable(&self) -> bool;

    /// Get recovery strategies for this error
    fn recovery_strategies(&self) -> Vec<RecoveryStrategy>;
}

/// Recovery strategies for handling errors
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Retry the operation
    Retry { max_attempts: usize, delay_ms: u64 },
    /// Use a fallback method
    Fallback { description: String },
    /// Reinitialize the component
    Reinitialize { component: String },
    /// Skip the current operation
    Skip { reason: String },
    /// Degrade functionality
    Degrade { description: String },
}

impl Recoverable for CaptureError {
    fn is_recoverable(&self) -> bool {
        self.context().recoverable
            || matches!(
                self,
                Self::Timeout { .. }
                    | Self::Network { .. }
                    | Self::Resource { .. }
                    | Self::Processing { .. }
                    | Self::Streaming { .. }
            )
    }

    fn recovery_strategies(&self) -> Vec<RecoveryStrategy> {
        match self {
            Self::Timeout { .. } => vec![
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    delay_ms: 1000,
                },
                RecoveryStrategy::Fallback {
                    description: "Use synchronous operation".to_string(),
                },
            ],
            Self::Network { .. } => vec![
                RecoveryStrategy::Retry {
                    max_attempts: 5,
                    delay_ms: 2000,
                },
                RecoveryStrategy::Reinitialize {
                    component: "network_connection".to_string(),
                },
            ],
            Self::Resource { .. } => vec![
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    delay_ms: 500,
                },
                RecoveryStrategy::Degrade {
                    description: "Reduce resource usage".to_string(),
                },
            ],
            Self::Processing { .. } => vec![
                RecoveryStrategy::Skip {
                    reason: "Skip current frame".to_string(),
                },
                RecoveryStrategy::Reinitialize {
                    component: "processing_pipeline".to_string(),
                },
            ],
            Self::Streaming { .. } => vec![
                RecoveryStrategy::Retry {
                    max_attempts: 3,
                    delay_ms: 1000,
                },
                RecoveryStrategy::Fallback {
                    description: "Switch to file output".to_string(),
                },
            ],
            _ => vec![],
        }
    }
}

/// Trait for errors with severity levels
pub trait HasSeverity {
    /// Get the severity level of this error
    fn severity(&self) -> ErrorSeverity;
}

impl HasSeverity for CaptureError {
    fn severity(&self) -> ErrorSeverity {
        self.context().severity
    }
}

/// Trait for errors that provide recovery suggestions
pub trait HasRecoverySuggestion {
    /// Get recovery suggestion for this error
    fn recovery_suggestion(&self) -> Option<&str>;
}

impl HasRecoverySuggestion for CaptureError {
    fn recovery_suggestion(&self) -> Option<&str> {
        self.context().recovery_suggestion.as_deref()
    }
}

/// Error classification utilities
pub mod classify {
    use super::*;

    /// Check if an error is transient (may resolve itself)
    pub fn is_transient(error: &CaptureError) -> bool {
        matches!(
            error,
            CaptureError::Timeout { .. }
                | CaptureError::Network { .. }
                | CaptureError::Resource { .. }
        )
    }

    /// Check if an error is fatal (cannot be recovered from)
    pub fn is_fatal(error: &CaptureError) -> bool {
        matches!(
            error,
            CaptureError::Config { .. }
                | CaptureError::Auth { .. }
                | CaptureError::Validation { .. }
        ) || error.severity() == ErrorSeverity::Fatal
    }

    /// Check if an error requires user intervention
    pub fn requires_user_intervention(error: &CaptureError) -> bool {
        error.severity() >= ErrorSeverity::Critical
    }

    /// Get error priority (higher numbers = higher priority)
    pub fn priority(error: &CaptureError) -> u8 {
        match error.severity() {
            ErrorSeverity::Debug => 0,
            ErrorSeverity::Info => 1,
            ErrorSeverity::Warning => 2,
            ErrorSeverity::Error => 3,
            ErrorSeverity::Critical => 4,
            ErrorSeverity::Fatal => 5,
        }
    }
}

/// Error conversion implementations
impl From<std::io::Error> for CaptureError {
    fn from(error: std::io::Error) -> Self {
        Self::io("unknown", error)
    }
}

#[cfg(feature = "gstreamer")]
impl From<gstreamer::glib::Error> for CaptureError {
    fn from(error: gstreamer::glib::Error) -> Self {
        Self::gstreamer(None, error.to_string())
    }
}

impl From<serde_json::Error> for CaptureError {
    fn from(error: serde_json::Error) -> Self {
        Self::external("serde_json", error)
    }
}

impl From<std::num::ParseIntError> for CaptureError {
    fn from(error: std::num::ParseIntError) -> Self {
        Self::validation("integer", "invalid format", error.to_string())
    }
}

impl From<std::num::ParseFloatError> for CaptureError {
    fn from(error: std::num::ParseFloatError) -> Self {
        Self::validation("float", "invalid format", error.to_string())
    }
}

/// Error builder for fluent error construction
pub struct ErrorBuilder {
    error: CaptureError,
}

impl ErrorBuilder {
    /// Create a new error builder
    pub fn new(error: CaptureError) -> Self {
        Self { error }
    }

    /// Add context
    pub fn context(mut self, context: impl Into<String>) -> Self {
        self.error = self.error.with_context(context);
        self
    }

    /// Add operation
    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.error = self.error.with_operation(operation);
        self
    }

    /// Add recovery suggestion
    pub fn recovery_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.error = self.error.with_recovery_suggestion(suggestion);
        self
    }

    /// Set severity
    pub fn severity(mut self, severity: ErrorSeverity) -> Self {
        self.error = self.error.with_severity(severity);
        self
    }

    /// Mark as retryable
    pub fn retryable(mut self) -> Self {
        self.error = self.error.retryable();
        self
    }

    /// Mark as recoverable
    pub fn recoverable(mut self) -> Self {
        self.error = self.error.recoverable();
        self
    }

    /// Add metadata
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.error = self.error.with_metadata(key, value);
        self
    }

    /// Build the error
    pub fn build(self) -> CaptureError {
        self.error
    }
}

/// Convenience macro for creating errors with context
#[macro_export]
macro_rules! capture_error {
    ($variant:ident, $($args:expr),* $(,)?) => {
        $crate::error::CaptureError::$variant($($args),*)
    };
    ($variant:ident { $($field:ident: $value:expr),* $(,)? }) => {
        $crate::error::CaptureError::$variant {
            $($field: $value,)*
            context: $crate::error::ErrorContext::new(),
        }
    };
}

/// Convenience macro for creating errors with context and additional properties
#[macro_export]
macro_rules! capture_error_with {
    ($base:expr) => {
        $crate::error::ErrorBuilder::new($base)
    };
    ($base:expr, $($method:ident: $value:expr),* $(,)?) => {
        $crate::error::ErrorBuilder::new($base)
            $(.$method($value))*
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let error = CaptureError::config("fps", "0", "must be greater than 0");
        assert_eq!(error.category(), "config");
        assert!(!error.is_retryable());
    }

    #[test]
    fn test_error_with_context() {
        let error = CaptureError::processing("resize", "invalid dimensions")
            .with_context("resizing frame for display")
            .with_recovery_suggestion("check frame dimensions before resize")
            .retryable();

        assert_eq!(error.category(), "processing");
        assert!(error.is_retryable());
        assert_eq!(
            error.recovery_suggestion(),
            Some("check frame dimensions before resize")
        );
    }

    #[test]
    fn test_error_traits() {
        let timeout_error = CaptureError::timeout("network_request", 5000).retryable();
        assert!(timeout_error.is_retryable());
        assert_eq!(timeout_error.retry_delay_ms(), Some(1000));
        assert_eq!(timeout_error.max_retries(), Some(3));
    }

    #[test]
    fn test_error_classification() {
        let config_error = CaptureError::config("invalid", "value", "reason");
        assert!(classify::is_fatal(&config_error));

        let network_error = CaptureError::network("connect");
        assert!(classify::is_transient(&network_error));
    }
}
