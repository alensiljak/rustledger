//! Beancount file loader with include resolution.
//!
//! This crate handles loading beancount files, resolving includes,
//! and collecting options. It builds on the parser to provide a
//! complete loading pipeline.
//!
//! # Features
//!
//! - Recursive include resolution with cycle detection
//! - Options collection and parsing
//! - Plugin directive collection
//! - Source map for error reporting
//! - Push/pop tag and metadata handling
//! - Automatic GPG decryption for encrypted files (`.gpg`, `.asc`)
//!
//! # Example
//!
//! ```ignore
//! use rustledger_loader::Loader;
//! use std::path::Path;
//!
//! let result = Loader::new().load(Path::new("ledger.beancount"))?;
//! for directive in result.directives {
//!     println!("{:?}", directive);
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "cache")]
pub mod cache;
mod options;
mod source_map;

#[cfg(feature = "cache")]
pub use cache::{
    CacheEntry, CachedOptions, CachedPlugin, invalidate_cache, load_cache_entry,
    reintern_directives, save_cache_entry,
};
pub use options::Options;
pub use source_map::{SourceFile, SourceMap};

use rustledger_core::{Directive, DisplayContext};
use rustledger_parser::{ParseError, Span, Spanned};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

/// Try to canonicalize a path, falling back to making it absolute if canonicalize
/// is not supported (e.g., on WASI).
///
/// This function:
/// 1. First tries `fs::canonicalize()` which resolves symlinks and returns absolute path
/// 2. If that fails (e.g., WASI doesn't support it), tries to make an absolute path manually
/// 3. As a last resort, returns the original path
fn normalize_path(path: &Path) -> PathBuf {
    // Try canonicalize first (works on most platforms, resolves symlinks)
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    // Fallback: make absolute without resolving symlinks (WASI-compatible)
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        // Join with current directory and clean up the path
        let mut result = cwd;
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::Normal(s) => {
                    result.push(s);
                }
                std::path::Component::CurDir => {}
                std::path::Component::RootDir => {
                    result = PathBuf::from("/");
                }
                std::path::Component::Prefix(p) => {
                    result = PathBuf::from(p.as_os_str());
                }
            }
        }
        result
    } else {
        // Last resort: just return the path as-is
        path.to_path_buf()
    }
}

/// Errors that can occur during loading.
#[derive(Debug, Error)]
pub enum LoadError {
    /// IO error reading a file.
    #[error("failed to read file {path}: {source}")]
    Io {
        /// The path that failed to read.
        path: PathBuf,
        /// The underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Include cycle detected.
    #[error("include cycle detected: {}", .cycle.join(" -> "))]
    IncludeCycle {
        /// The cycle of file paths.
        cycle: Vec<String>,
    },

    /// Parse errors occurred.
    #[error("parse errors in {path}")]
    ParseErrors {
        /// The file with parse errors.
        path: PathBuf,
        /// The parse errors.
        errors: Vec<ParseError>,
    },

    /// Path traversal attempt detected.
    #[error("path traversal not allowed: {include_path} escapes base directory {base_dir}")]
    PathTraversal {
        /// The include path that attempted traversal.
        include_path: String,
        /// The base directory.
        base_dir: PathBuf,
    },

    /// GPG decryption failed.
    #[error("failed to decrypt {path}: {message}")]
    Decryption {
        /// The encrypted file path.
        path: PathBuf,
        /// Error message from GPG.
        message: String,
    },
}

/// Result of loading a beancount file.
#[derive(Debug)]
pub struct LoadResult {
    /// All directives from all files, in order.
    pub directives: Vec<Spanned<Directive>>,
    /// Parsed options.
    pub options: Options,
    /// Plugins to load.
    pub plugins: Vec<Plugin>,
    /// Source map for error reporting.
    pub source_map: SourceMap,
    /// All errors encountered during loading.
    pub errors: Vec<LoadError>,
    /// Display context for formatting numbers (tracks precision per currency).
    pub display_context: DisplayContext,
}

/// A plugin directive.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// Plugin module name (with any `python:` prefix stripped).
    pub name: String,
    /// Optional configuration string.
    pub config: Option<String>,
    /// Source location.
    pub span: Span,
    /// File this plugin was declared in.
    pub file_id: usize,
    /// Whether the `python:` prefix was used to force Python execution.
    pub force_python: bool,
}

/// Check if a file is GPG-encrypted based on extension or content.
///
/// Returns `true` for:
/// - Files with `.gpg` extension
/// - Files with `.asc` extension containing a PGP message header
fn is_encrypted_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("gpg") => true,
        Some("asc") => {
            // Check for PGP header in first 1024 bytes
            if let Ok(content) = fs::read_to_string(path) {
                let check_len = 1024.min(content.len());
                content[..check_len].contains("-----BEGIN PGP MESSAGE-----")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Decrypt a GPG-encrypted file using the system `gpg` command.
///
/// This uses `gpg --batch --decrypt` which will use the user's
/// GPG keyring and gpg-agent for passphrase handling.
fn decrypt_gpg_file(path: &Path) -> Result<String, LoadError> {
    let output = Command::new("gpg")
        .args(["--batch", "--decrypt"])
        .arg(path)
        .output()
        .map_err(|e| LoadError::Decryption {
            path: path.to_path_buf(),
            message: format!("failed to run gpg: {e}"),
        })?;

    if !output.status.success() {
        return Err(LoadError::Decryption {
            path: path.to_path_buf(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|e| LoadError::Decryption {
        path: path.to_path_buf(),
        message: format!("decrypted content is not valid UTF-8: {e}"),
    })
}

/// Beancount file loader.
#[derive(Debug, Default)]
pub struct Loader {
    /// Files that have been loaded (for cycle detection).
    loaded_files: HashSet<PathBuf>,
    /// Stack for cycle detection during loading (maintains order for error messages).
    include_stack: Vec<PathBuf>,
    /// Set for O(1) cycle detection (mirrors `include_stack`).
    include_stack_set: HashSet<PathBuf>,
    /// Root directory for path traversal protection.
    /// If set, includes must resolve to paths within this directory.
    root_dir: Option<PathBuf>,
    /// Whether to enforce path traversal protection.
    enforce_path_security: bool,
}

impl Loader {
    /// Create a new loader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable path traversal protection.
    ///
    /// When enabled, include directives cannot escape the root directory
    /// of the main beancount file. This prevents malicious ledger files
    /// from accessing sensitive files outside the ledger directory.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = Loader::new()
    ///     .with_path_security(true)
    ///     .load(Path::new("ledger.beancount"))?;
    /// ```
    #[must_use]
    pub const fn with_path_security(mut self, enabled: bool) -> Self {
        self.enforce_path_security = enabled;
        self
    }

    /// Set a custom root directory for path security.
    ///
    /// By default, the root directory is the parent directory of the main file.
    /// This method allows overriding that to a custom directory.
    #[must_use]
    pub fn with_root_dir(mut self, root: PathBuf) -> Self {
        self.root_dir = Some(root);
        self.enforce_path_security = true;
        self
    }

    /// Load a beancount file and all its includes.
    ///
    /// Parses the file, processes options and plugin directives, and recursively
    /// loads any included files.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] in the following cases:
    ///
    /// - [`LoadError::Io`] - Failed to read the file or an included file
    /// - [`LoadError::IncludeCycle`] - Circular include detected
    ///
    /// Note: Parse errors and path traversal errors are collected in
    /// [`LoadResult::errors`] rather than returned directly, allowing
    /// partial results to be returned.
    pub fn load(&mut self, path: &Path) -> Result<LoadResult, LoadError> {
        let mut directives = Vec::new();
        let mut options = Options::default();
        let mut plugins = Vec::new();
        let mut source_map = SourceMap::new();
        let mut errors = Vec::new();

        // Get normalized absolute path (WASI-compatible, doesn't require canonicalize)
        let canonical = normalize_path(path);

        // Set root directory for path security if enabled but not explicitly set
        if self.enforce_path_security && self.root_dir.is_none() {
            self.root_dir = canonical.parent().map(Path::to_path_buf);
        }

        self.load_recursive(
            &canonical,
            &mut directives,
            &mut options,
            &mut plugins,
            &mut source_map,
            &mut errors,
        )?;

        // Build display context from directives and options
        let display_context = build_display_context(&directives, &options);

        Ok(LoadResult {
            directives,
            options,
            plugins,
            source_map,
            errors,
            display_context,
        })
    }

    fn load_recursive(
        &mut self,
        path: &Path,
        directives: &mut Vec<Spanned<Directive>>,
        options: &mut Options,
        plugins: &mut Vec<Plugin>,
        source_map: &mut SourceMap,
        errors: &mut Vec<LoadError>,
    ) -> Result<(), LoadError> {
        // Allocate path once for reuse
        let path_buf = path.to_path_buf();

        // Check for cycles using O(1) HashSet lookup
        if self.include_stack_set.contains(&path_buf) {
            let mut cycle: Vec<String> = self
                .include_stack
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            cycle.push(path.display().to_string());
            return Err(LoadError::IncludeCycle { cycle });
        }

        // Check if already loaded
        if self.loaded_files.contains(&path_buf) {
            return Ok(());
        }

        // Read file (decrypting if necessary)
        // Try fast UTF-8 conversion first, fall back to lossy for non-UTF-8 files
        let source: std::sync::Arc<str> = if is_encrypted_file(path) {
            decrypt_gpg_file(path)?.into()
        } else {
            let bytes = fs::read(path).map_err(|e| LoadError::Io {
                path: path_buf.clone(),
                source: e,
            })?;
            // Try zero-copy conversion first (common case), fall back to lossy
            match String::from_utf8(bytes) {
                Ok(s) => s.into(),
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned().into(),
            }
        };

        // Add to source map (Arc::clone is cheap - just increments refcount)
        let file_id = source_map.add_file(path_buf.clone(), std::sync::Arc::clone(&source));

        // Mark as loading (update both stack and set)
        self.include_stack_set.insert(path_buf.clone());
        self.include_stack.push(path_buf.clone());
        self.loaded_files.insert(path_buf);

        // Parse (borrows from Arc, no allocation)
        let result = rustledger_parser::parse(&source);

        // Collect parse errors
        if !result.errors.is_empty() {
            errors.push(LoadError::ParseErrors {
                path: path.to_path_buf(),
                errors: result.errors,
            });
        }

        // Process options
        for (key, value, _span) in result.options {
            options.set(&key, &value);
        }

        // Process plugins
        for (name, config, span) in result.plugins {
            // Check for "python:" prefix to force Python execution
            let (actual_name, force_python) = if let Some(stripped) = name.strip_prefix("python:") {
                (stripped.to_string(), true)
            } else {
                (name, false)
            };
            plugins.push(Plugin {
                name: actual_name,
                config,
                span,
                file_id,
                force_python,
            });
        }

        // Process includes
        let base_dir = path.parent().unwrap_or(Path::new("."));
        for (include_path, _span) in &result.includes {
            let full_path = base_dir.join(include_path);
            // Use normalize_path for WASI compatibility (canonicalize not supported)
            let canonical = normalize_path(&full_path);

            // Path traversal protection: ensure include stays within root directory
            if self.enforce_path_security
                && let Some(ref root) = self.root_dir
                && !canonical.starts_with(root)
            {
                errors.push(LoadError::PathTraversal {
                    include_path: include_path.clone(),
                    base_dir: root.clone(),
                });
                continue;
            }

            if let Err(e) =
                self.load_recursive(&canonical, directives, options, plugins, source_map, errors)
            {
                errors.push(e);
            }
        }

        // Add directives from this file, setting the file_id
        directives.extend(
            result
                .directives
                .into_iter()
                .map(|d| d.with_file_id(file_id)),
        );

        // Pop from stack and set
        if let Some(popped) = self.include_stack.pop() {
            self.include_stack_set.remove(&popped);
        }

        Ok(())
    }
}

/// Build a display context from loaded directives and options.
///
/// This scans all directives for amounts and tracks the maximum precision seen
/// for each currency. Fixed precisions from `option "display_precision"` override
/// the inferred values.
fn build_display_context(directives: &[Spanned<Directive>], options: &Options) -> DisplayContext {
    let mut ctx = DisplayContext::new();

    // Set render_commas from options
    ctx.set_render_commas(options.render_commas);

    // Scan directives for amounts to infer precision
    for spanned in directives {
        match &spanned.value {
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    // Units (IncompleteAmount)
                    if let Some(ref units) = posting.units
                        && let (Some(number), Some(currency)) = (units.number(), units.currency())
                    {
                        ctx.update(number, currency);
                    }
                    // Cost (CostSpec)
                    if let Some(ref cost) = posting.cost
                        && let (Some(number), Some(currency)) =
                            (cost.number_per.or(cost.number_total), &cost.currency)
                    {
                        ctx.update(number, currency.as_str());
                    }
                    // Price (PriceAnnotation)
                    if let Some(ref price) = posting.price
                        && let Some(amount) = price.amount()
                    {
                        ctx.update(amount.number, amount.currency.as_str());
                    }
                }
            }
            Directive::Balance(bal) => {
                ctx.update(bal.amount.number, bal.amount.currency.as_str());
                if let Some(tol) = bal.tolerance {
                    ctx.update(tol, bal.amount.currency.as_str());
                }
            }
            Directive::Price(price) => {
                ctx.update(price.amount.number, price.amount.currency.as_str());
            }
            Directive::Pad(_)
            | Directive::Open(_)
            | Directive::Close(_)
            | Directive::Commodity(_)
            | Directive::Event(_)
            | Directive::Query(_)
            | Directive::Note(_)
            | Directive::Document(_)
            | Directive::Custom(_) => {}
        }
    }

    // Apply fixed precisions from options (these override inferred values)
    for (currency, precision) in &options.display_precision {
        ctx.set_fixed_precision(currency, *precision);
    }

    ctx
}

/// Load a beancount file.
///
/// This is a convenience function that creates a loader and loads a single file.
pub fn load(path: &Path) -> Result<LoadResult, LoadError> {
    Loader::new().load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_encrypted_file_gpg_extension() {
        let path = Path::new("test.beancount.gpg");
        assert!(is_encrypted_file(path));
    }

    #[test]
    fn test_is_encrypted_file_plain_beancount() {
        let path = Path::new("test.beancount");
        assert!(!is_encrypted_file(path));
    }

    #[test]
    fn test_is_encrypted_file_asc_with_pgp_header() {
        let mut file = NamedTempFile::with_suffix(".asc").unwrap();
        writeln!(file, "-----BEGIN PGP MESSAGE-----").unwrap();
        writeln!(file, "some encrypted content").unwrap();
        writeln!(file, "-----END PGP MESSAGE-----").unwrap();
        file.flush().unwrap();

        assert!(is_encrypted_file(file.path()));
    }

    #[test]
    fn test_is_encrypted_file_asc_without_pgp_header() {
        let mut file = NamedTempFile::with_suffix(".asc").unwrap();
        writeln!(file, "This is just a plain text file").unwrap();
        writeln!(file, "with .asc extension but no PGP content").unwrap();
        file.flush().unwrap();

        assert!(!is_encrypted_file(file.path()));
    }

    #[test]
    fn test_decrypt_gpg_file_missing_gpg() {
        // Create a fake .gpg file
        let mut file = NamedTempFile::with_suffix(".gpg").unwrap();
        writeln!(file, "fake encrypted content").unwrap();
        file.flush().unwrap();

        // This will fail because the content isn't actually GPG-encrypted
        // (or gpg isn't installed, or there's no matching key)
        let result = decrypt_gpg_file(file.path());
        assert!(result.is_err());

        if let Err(LoadError::Decryption { path, message }) = result {
            assert_eq!(path, file.path().to_path_buf());
            assert!(!message.is_empty());
        } else {
            panic!("Expected Decryption error");
        }
    }
}
