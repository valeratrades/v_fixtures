//! Test fixture utilities for defining multi-file test cases inline.
//!
//! This crate provides a way to define file trees inline in test code using
//! the `//- /path.rs` syntax inspired by rust-analyzer.
//!
//! # Single file fixture
//!
//! ```
//! use v_fixtures::Fixture;
//!
//! let fixture = Fixture::parse(r#"
//!     fn main() {
//!         println!("Hello World")
//!     }
//! "#);
//! assert_eq!(fixture.files.len(), 1);
//! assert_eq!(fixture.files[0].path, "/main.rs");
//! ```
//!
//! # Multi-file fixture
//!
//! ```
//! use v_fixtures::Fixture;
//!
//! let fixture = Fixture::parse(r#"
//!     //- /main.rs
//!     mod foo;
//!     fn main() { foo::bar(); }
//!
//!     //- /foo.rs
//!     pub fn bar() {}
//! "#);
//! assert_eq!(fixture.files.len(), 2);
//! ```
//!
//! # Writing to temp directory
//!
//! ```
//! use v_fixtures::Fixture;
//!
//! let fixture = Fixture::parse(r#"
//!     //- /src/main.rs
//!     fn main() {}
//!     //- /src/lib.rs
//!     pub fn hello() {}
//! "#);
//! let temp = fixture.write_to_tempdir();
//! assert!(temp.path("/src/main.rs").exists());
//! assert!(temp.path("/src/lib.rs").exists());
//! ```
//!
//! # Testing with insta snapshots
//!
//! ```ignore
//! use v_fixtures::Fixture;
//!
//! // Apply some transformation to files...
//! let temp = fixture.write_to_tempdir();
//! // ... run your tool ...
//! let result = temp.read_all_from_disk();
//! insta::assert_snapshot!(result.render(), @"...");
//! ```

use std::{borrow::Cow, fs, path::PathBuf};

/// A single file in a fixture
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureFile {
	/// Path relative to fixture root (e.g., "/main.rs" or "/tests/test.rs")
	pub path: String,
	/// File contents with meta lines stripped
	pub text: String,
}

/// Parsed fixture containing multiple files
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Fixture {
	pub files: Vec<FixtureFile>,
}

impl Fixture {
	/// Read all files from a directory into a Fixture.
	///
	/// This walks the directory recursively, skipping `.git` directories,
	/// and creates a Fixture with all text files found. Files are sorted
	/// by path for deterministic output.
	///
	/// # Arguments
	///
	/// * `path` - The directory to read from
	///
	/// # Returns
	///
	/// Returns `None` if the directory doesn't exist, or `Some(Fixture)` with
	/// an empty files vec if the directory exists but contains no readable files.
	///
	/// # Example
	///
	/// ```ignore
	/// use v_fixtures::Fixture;
	///
	/// let fixture = Fixture::read_from_directory("/path/to/dir").unwrap();
	/// insta::assert_snapshot!(fixture.render());
	/// ```
	pub fn read_from_directory(path: impl AsRef<std::path::Path>) -> Option<Self> {
		let path = path.as_ref();
		if !path.exists() {
			return None;
		}

		let mut files = Vec::new();
		for entry in walkdir::WalkDir::new(path)
			.into_iter()
			.filter_entry(|e| !e.path().to_string_lossy().contains(".git"))
			.filter_map(Result::ok)
		{
			let entry_path = entry.path();
			if entry_path.is_file() {
				let relative_path = entry_path.strip_prefix(path).expect("path should be under base");
				let relative_str = format!("/{}", relative_path.to_string_lossy());
				if let Ok(text) = fs::read_to_string(entry_path) {
					files.push(FixtureFile { path: relative_str, text });
				}
			}
		}

		files.sort_by(|a, b| a.path.cmp(&b.path));
		Some(Self { files })
	}

	/// Parse a fixture string into files.
	///
	/// Supports the `//- /path.rs` syntax for multi-file fixtures.
	/// If no file markers are present, treats the whole string as a single `/main.rs` file.
	pub fn parse(fixture: &str) -> Self {
		Self::parse_with_default_path(fixture, "/main.rs")
	}

	/// Parse a fixture string with a custom default path for single-file fixtures.
	pub fn parse_with_default_path(fixture: &str, default_path: &str) -> Self {
		let fixture = trim_indent(fixture);
		let fixture = fixture.as_str();

		let mut files = Vec::new();

		if !fixture.contains("//-") {
			// Single file fixture - treat as default path
			return Self {
				files: vec![FixtureFile {
					path: default_path.to_owned(),
					text: fixture.to_owned(),
				}],
			};
		}

		let mut current_path: Option<String> = None;
		let mut current_text = String::new();

		for line in fixture.split_inclusive('\n') {
			if let Some(rest) = line.strip_prefix("//-") {
				// Save previous file if any
				if let Some(path) = current_path.take() {
					files.push(FixtureFile {
						path,
						text: std::mem::take(&mut current_text),
					});
				}

				// Parse new file path
				let meta = rest.trim();
				let path = meta.split_whitespace().next().expect("fixture meta must have a path");
				assert!(path.starts_with('/'), "fixture path must start with `/`: {path:?}");
				current_path = Some(path.to_owned());
			} else if current_path.is_some() {
				current_text.push_str(line);
			}
		}

		// Save last file
		if let Some(path) = current_path {
			files.push(FixtureFile { path, text: current_text });
		}

		Self { files }
	}

	/// Write fixture files to a temporary directory and return the path
	pub fn write_to_tempdir(&self) -> TempFixture {
		self.write_to_tempdir_with_prefix("v_fixture_")
	}

	/// Write fixture files to a temporary directory with a custom prefix
	pub fn write_to_tempdir_with_prefix(&self, prefix: &str) -> TempFixture {
		let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().expect("failed to create temp dir");

		for file in &self.files {
			let path = temp_dir.path().join(file.path.trim_start_matches('/'));
			if let Some(parent) = path.parent() {
				fs::create_dir_all(parent).expect("failed to create parent dirs");
			}
			fs::write(&path, &file.text).expect("failed to write fixture file");
		}

		TempFixture {
			root: temp_dir.path().to_path_buf(),
			temp_dir,
			files: self.files.clone(),
			cwd: None,
		}
	}

	/// Get a file by path
	pub fn file(&self, path: &str) -> Option<&FixtureFile> {
		self.files.iter().find(|f| f.path == path)
	}

	/// Get a mutable file by path
	pub fn file_mut(&mut self, path: &str) -> Option<&mut FixtureFile> {
		self.files.iter_mut().find(|f| f.path == path)
	}

	/// Get the single file (panics if multiple files)
	pub fn single_file(&self) -> &FixtureFile {
		assert_eq!(self.files.len(), 1, "expected single file fixture, got {}", self.files.len());
		&self.files[0]
	}

	/// Check if fixture contains a file at the given path
	pub fn contains(&self, path: &str) -> bool {
		self.files.iter().any(|f| f.path == path)
	}

	/// Render the fixture to a string format (for snapshots).
	///
	/// Single-file fixtures render as just the content.
	/// Multi-file fixtures render with `//- /path` markers.
	///
	/// For more control over rendering (line redaction, git hash normalization),
	/// use [`FixtureRenderer`].
	///
	/// # Example
	///
	/// ```
	/// use v_fixtures::Fixture;
	///
	/// let fixture = Fixture::parse(r#"
	/// //- /a.rs
	/// fn a() {}
	/// //- /b.rs
	/// fn b() {}
	/// "#);
	/// let output = fixture.render();
	/// assert!(output.contains("//- /a.rs"));
	/// ```
	pub fn render(&self) -> String {
		FixtureRenderer::new(self).render()
	}
}

/// A fixture written to a temporary directory
#[derive(derive_new::new)]
pub struct TempFixture {
	/// Root path of the temporary directory
	pub root: PathBuf,
	/// The temp directory handle - kept alive to preserve the directory
	pub temp_dir: tempfile::TempDir,
	/// Original files that were written
	pub files: Vec<FixtureFile>,
	/// Current working directory for path resolution (relative to root).
	/// Defaults to root. Used by `read_all_from_disk` to determine which
	/// paths to include and how to format them.
	#[new(default)]
	cwd: Option<PathBuf>,
}

impl TempFixture {
	/// Set the current working directory for path resolution.
	///
	/// When `read_all_from_disk` is called, only files under this directory
	/// will be included, and paths will be relative to it.
	///
	/// # Example
	///
	/// ```
	/// use v_fixtures::Fixture;
	///
	/// let fixture = Fixture::parse(r#"
	/// //- /data/app/file.txt
	/// content
	/// //- /cache/temp.txt
	/// temp
	/// "#);
	/// let temp = fixture.write_to_tempdir().cwd("data/app");
	///
	/// // Only files under data/app are included
	/// let result = temp.read_all_from_disk();
	/// assert_eq!(result.files.len(), 1);
	/// assert_eq!(result.files[0].path, "/file.txt");
	/// ```
	pub fn cwd(mut self, path: &str) -> Self {
		self.cwd = Some(PathBuf::from(path.trim_start_matches('/')));
		self
	}

	/// Get the effective cwd path (absolute)
	fn effective_cwd(&self) -> PathBuf {
		match &self.cwd {
			Some(cwd) => self.root.join(cwd),
			None => self.root.clone(),
		}
	}

	/// Get the full path to a file
	pub fn path(&self, relative: &str) -> PathBuf {
		self.root.join(relative.trim_start_matches('/'))
	}

	/// Read a file's current contents
	pub fn read(&self, relative: &str) -> String {
		fs::read_to_string(self.path(relative)).expect("failed to read file")
	}

	/// Try to read a file's current contents
	pub fn try_read(&self, relative: &str) -> Option<String> {
		fs::read_to_string(self.path(relative)).ok()
	}

	/// Write content to a file (creates parent dirs if needed)
	pub fn write(&self, relative: &str, content: &str) {
		let path = self.path(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		fs::write(path, content).expect("failed to write file");
	}

	/// Create a named pipe (FIFO) at the given path.
	///
	/// Useful for mocking interactive processes in tests where you need
	/// to signal when a "blocking" operation should complete.
	///
	/// # Example
	///
	/// ```ignore
	/// use std::io::Write;
	/// let temp = Fixture::parse("").write_to_tempdir();
	/// let pipe_path = temp.create_pipe("signal_pipe");
	///
	/// // In test: signal the pipe to unblock waiting process
	/// let mut pipe = std::fs::OpenOptions::new()
	///     .write(true)
	///     .open(&pipe_path)
	///     .unwrap();
	/// pipe.write_all(b"x").unwrap();
	/// ```
	#[cfg(unix)]
	pub fn create_pipe(&self, relative: &str) -> PathBuf {
		let path = self.path(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU).expect("failed to create named pipe");
		path
	}

	/// Read all original files and return as a new Fixture
	pub fn read_all(&self) -> Fixture {
		let files = self
			.files
			.iter()
			.map(|f| {
				let text = self.read(&f.path);
				FixtureFile { path: f.path.clone(), text }
			})
			.collect();
		Fixture { files }
	}

	/// Read all files from disk (discovering any new files or noting deleted ones)
	/// Returns files sorted by path for deterministic output.
	///
	/// If `cwd` is set, only files under that directory are included and paths
	/// are relative to it. Otherwise, all files under root are included.
	pub fn read_all_from_disk(&self) -> Fixture {
		let mut files: Vec<FixtureFile> = Vec::new();
		let base = self.effective_cwd();

		for entry in walkdir::WalkDir::new(&base).into_iter().filter_map(Result::ok) {
			let path = entry.path();
			if path.is_file() {
				let relative_path = path.strip_prefix(&base).expect("path should be under base");
				let relative_str = format!("/{}", relative_path.to_string_lossy());
				if let Ok(text) = fs::read_to_string(path) {
					files.push(FixtureFile { path: relative_str, text });
				}
			}
		}

		// Sort by path for deterministic output
		files.sort_by(|a, b| a.path.cmp(&b.path));
		Fixture { files }
	}
}

/// Parse a before/after fixture separated by `=>`
///
/// Returns (before_fixture, after_fixture)
///
/// # Example
///
/// ```
/// use v_fixtures::parse_before_after;
///
/// let (before, after) = parse_before_after(r#"
///     //- /test.rs
///     fn main() { let x = 1; }
///     =>
///     //- /test.rs
///     fn main() { let y = 1; }
/// "#);
/// assert!(before.files[0].text.contains("let x"));
/// assert!(after.files[0].text.contains("let y"));
/// ```
pub fn parse_before_after(fixture: &str) -> (Fixture, Fixture) {
	let fixture = trim_indent(fixture);
	let parts: Vec<&str> = fixture.split("\n=>\n").collect();
	assert_eq!(parts.len(), 2, "expected exactly one `=>` separator in before/after fixture");

	let before = Fixture::parse(parts[0]);
	let after = Fixture::parse(parts[1]);

	(before, after)
}

/// Remove common leading indentation from all lines.
///
/// This allows writing nicely indented fixture strings in tests.
pub fn trim_indent(text: &str) -> String {
	let mut text = text;
	if text.starts_with('\n') {
		text = &text[1..];
	}
	let indent = text.lines().filter(|it| !it.trim().is_empty()).map(|it| it.len() - it.trim_start().len()).min().unwrap_or(0);
	text.split_inclusive('\n')
		.map(|line| if line.len() <= indent { line.trim_start_matches(' ') } else { &line[indent..] })
		.collect()
}

/// Compare two fixtures for equality, with nice diff output on failure
#[track_caller]
pub fn assert_fixture_eq(expected: &Fixture, actual: &Fixture) {
	if expected.files.len() != actual.files.len() {
		panic!(
			"fixture file count mismatch: expected {} files, got {}\nExpected: {:?}\nActual: {:?}",
			expected.files.len(),
			actual.files.len(),
			expected.files.iter().map(|f| &f.path).collect::<Vec<_>>(),
			actual.files.iter().map(|f| &f.path).collect::<Vec<_>>()
		);
	}

	for expected_file in &expected.files {
		let actual_file = actual.file(&expected_file.path).unwrap_or_else(|| {
			panic!(
				"missing file in actual: {}\nActual files: {:?}",
				expected_file.path,
				actual.files.iter().map(|f| &f.path).collect::<Vec<_>>()
			)
		});

		if expected_file.text != actual_file.text {
			panic!(
				"file {} content mismatch:\n\n--- Expected ---\n{}\n--- Actual ---\n{}\n",
				expected_file.path, expected_file.text, actual_file.text
			);
		}
	}
}

pub mod fs_standards;

/// Builder for rendering fixtures with various normalizations.
///
/// # Example
///
/// ```ignore
/// use v_fixtures::{Fixture, FixtureRenderer};
///
/// let fixture = Fixture::read_from_directory("/path/to/dir").unwrap();
/// let output = FixtureRenderer::new(&fixture)
///     .normalize_git_hashes()
///     .redact_lines(&[20, 25])
///     .render();
/// insta::assert_snapshot!(output);
/// ```
pub struct FixtureRenderer<'a> {
	fixture: &'a Fixture,
	normalize_git_hashes: bool,
	lines_to_redact: Vec<usize>,
	redact_message: Cow<'static, str>,
	path_patterns: Vec<PathPattern>,
	always_show_filepath: bool,
}
impl<'a> FixtureRenderer<'a> {
	/// Create a new renderer for the given fixture.
	pub fn new(fixture: &'a Fixture) -> Self {
		Self {
			fixture,
			normalize_git_hashes: false,
			lines_to_redact: Vec::new(),
			redact_message: Cow::Borrowed("[REDACTED]"),
			path_patterns: Vec::new(),
			always_show_filepath: false,
		}
	}

	/// Normalize git commit hashes in diff3 conflict markers.
	///
	/// Replaces patterns like `||||||| a0f7d74` with `||||||| [hash]`.
	/// Useful for deterministic snapshots when testing git merge conflicts.
	pub fn normalize_git_hashes(mut self) -> Self {
		self.normalize_git_hashes = true;
		self
	}

	/// Redact specific lines from the output.
	///
	/// Line numbers are 1-indexed and refer to the final rendered output.
	/// Redacted lines are replaced with the redact message (default: "[REDACTED]").
	///
	/// Useful for non-deterministic values like timestamps.
	pub fn redact_lines(mut self, lines: &[usize]) -> Self {
		self.lines_to_redact = lines.to_vec();
		self
	}

	/// Set a custom message for redacted lines.
	///
	/// Default is "[REDACTED]".
	pub fn redact_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
		self.redact_message = message.into();
		self
	}

	/// Always show filepath headers, even for single-file fixtures.
	///
	/// By default, single-file fixtures render without the `//- path` header.
	/// This forces the header to always be included.
	pub fn always_show_filepath(mut self) -> Self {
		self.always_show_filepath = true;
		self
	}

	/// Filter files by path using a regex pattern.
	///
	/// The pattern is matched as a substring against file paths.
	/// Prefix with `!` to exclude matching files instead.
	///
	/// Multiple calls accumulate patterns. Files must match at least one
	/// inclusion pattern (if any) and must not match any exclusion pattern.
	///
	/// # Example
	///
	/// ```
	/// use v_fixtures::{Fixture, FixtureRenderer};
	///
	/// let fixture = Fixture::parse(r#"
	/// //- /src/main.rs
	/// fn main() {}
	/// //- /src/lib.rs
	/// pub fn lib() {}
	/// //- /tests/test.rs
	/// fn test() {}
	/// "#);
	///
	/// // Include only src files
	/// let output = FixtureRenderer::new(&fixture)
	///     .regex("^/src/")
	///     .render();
	/// assert!(output.contains("/src/main.rs"));
	/// assert!(!output.contains("/tests/"));
	///
	/// // Exclude test files
	/// let output = FixtureRenderer::new(&fixture)
	///     .regex("!test")
	///     .render();
	/// assert!(output.contains("/src/main.rs"));
	/// assert!(!output.contains("/tests/"));
	/// ```
	pub fn regex(mut self, pattern: &str) -> Self {
		let (pattern, exclude) = match pattern.strip_prefix('!') {
			Some(rest) => (rest, true),
			None => (pattern, false),
		};
		let regex = regex::Regex::new(pattern).expect("invalid regex pattern");
		self.path_patterns.push(PathPattern { regex, exclude });
		self
	}

	/// Render the fixture to a string.
	pub fn render(self) -> String {
		let mut result = self.render_raw();

		// Apply git hash normalization
		if self.normalize_git_hashes {
			// Regex to match git commit hashes in diff3 conflict markers (e.g., "||||||| a0f7d74")
			let hash_regex = regex::Regex::new(r"\|\|\|\|\|\|\| [0-9a-f]{7,40}").unwrap();
			result = hash_regex.replace_all(&result, "||||||| [hash]").to_string();
		}

		// Apply line redaction
		if !self.lines_to_redact.is_empty() {
			result = result
				.lines()
				.enumerate()
				.map(|(i, line)| {
					let line_num = i + 1; // 1-indexed
					if self.lines_to_redact.contains(&line_num) {
						self.redact_message.to_string()
					} else {
						line.to_string()
					}
				})
				.collect::<Vec<_>>()
				.join("\n");
		}

		result
	}

	/// Render without any post-processing (no normalization or redaction).
	fn render_raw(&self) -> String {
		let files: Vec<_> = self.fixture.files.iter().filter(|f| self.matches_path(&f.path)).collect();

		if files.len() == 1 && !self.always_show_filepath {
			return files[0].text.clone();
		}

		let mut result = String::new();
		for file in files {
			result.push_str("//- ");
			result.push_str(&file.path);
			result.push('\n');
			result.push_str(&file.text);
			if !file.text.ends_with('\n') {
				result.push('\n');
			}
		}
		result
	}

	/// Check if a path matches the configured patterns.
	///
	/// Returns true if:
	/// - No patterns are configured, OR
	/// - Path matches at least one inclusion pattern (if any) AND does not match any exclusion pattern
	fn matches_path(&self, path: &str) -> bool {
		if self.path_patterns.is_empty() {
			return true;
		}

		let inclusions: Vec<_> = self.path_patterns.iter().filter(|p| !p.exclude).collect();
		let exclusions: Vec<_> = self.path_patterns.iter().filter(|p| p.exclude).collect();

		// If there are exclusion patterns and path matches any, reject
		if exclusions.iter().any(|p| p.regex.is_match(path)) {
			return false;
		}

		// If there are inclusion patterns, path must match at least one
		if !inclusions.is_empty() {
			return inclusions.iter().any(|p| p.regex.is_match(path));
		}

		true
	}
}

struct PathPattern {
	regex: regex::Regex,
	exclude: bool,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_trim_indent() {
		let input = r#"
            fn main() {
                println!("hello");
            }
        "#;
		let expected = "fn main() {\n    println!(\"hello\");\n}\n";
		assert_eq!(trim_indent(input), expected);
	}

	#[test]
	fn test_parse_single_file() {
		let input = r#"
            fn main() {
                println!("hello");
            }
        "#;
		let fixture = Fixture::parse(input);
		assert_eq!(fixture.files.len(), 1);
		assert_eq!(fixture.files[0].path, "/main.rs");
		assert!(fixture.files[0].text.contains("fn main()"));
	}

	#[test]
	fn test_parse_multi_file() {
		let input = r#"
            //- /main.rs
            mod foo;
            fn main() { foo::bar(); }

            //- /foo.rs
            pub fn bar() {}
        "#;
		let fixture = Fixture::parse(input);
		assert_eq!(fixture.files.len(), 2);
		assert_eq!(fixture.files[0].path, "/main.rs");
		assert!(fixture.files[0].text.contains("mod foo"));
		assert_eq!(fixture.files[1].path, "/foo.rs");
		assert!(fixture.files[1].text.contains("pub fn bar"));
	}

	#[test]
	fn test_parse_nested_paths() {
		let input = r#"
            //- /src/main.rs
            mod lib;

            //- /tests/test.rs
            fn test() {}
        "#;
		let fixture = Fixture::parse(input);
		assert_eq!(fixture.files.len(), 2);
		assert_eq!(fixture.files[0].path, "/src/main.rs");
		assert_eq!(fixture.files[1].path, "/tests/test.rs");
	}

	#[test]
	fn test_parse_before_after() {
		let input = r#"
            //- /test.rs
            fn main() { let x = 1; }
            =>
            //- /test.rs
            fn main() { let y = 1; }
        "#;
		let (before, after) = parse_before_after(input);
		assert!(before.files[0].text.contains("let x"));
		assert!(after.files[0].text.contains("let y"));
	}

	#[test]
	fn test_render_single() {
		let fixture = Fixture {
			files: vec![FixtureFile {
				path: "/main.rs".to_owned(),
				text: "fn main() {}\n".to_owned(),
			}],
		};
		let rendered = fixture.render();
		assert_eq!(rendered, "fn main() {}\n");
	}

	#[test]
	fn test_render_multi() {
		let fixture = Fixture {
			files: vec![
				FixtureFile {
					path: "/main.rs".to_owned(),
					text: "fn main() {}\n".to_owned(),
				},
				FixtureFile {
					path: "/lib.rs".to_owned(),
					text: "pub fn lib() {}\n".to_owned(),
				},
			],
		};
		let rendered = fixture.render();
		assert!(rendered.contains("//- /main.rs"));
		assert!(rendered.contains("//- /lib.rs"));
	}

	#[test]
	fn test_write_and_read_tempdir() {
		let fixture = Fixture::parse(
			r#"
            //- /src/main.rs
            fn main() {}
            //- /src/lib.rs
            pub fn hello() {}
        "#,
		);
		let temp = fixture.write_to_tempdir();

		assert!(temp.path("/src/main.rs").exists());
		assert!(temp.path("/src/lib.rs").exists());
		assert!(temp.read("/src/main.rs").contains("fn main()"));
		assert!(temp.read("/src/lib.rs").contains("pub fn hello()"));
	}

	#[test]
	fn test_read_all_from_disk() {
		let fixture = Fixture::parse(
			r#"
            //- /a.rs
            a
            //- /b.rs
            b
        "#,
		);
		let temp = fixture.write_to_tempdir();

		// Add a new file
		temp.write("/c.rs", "c\n");

		let result = temp.read_all_from_disk();
		assert_eq!(result.files.len(), 3);
		assert!(result.contains("/a.rs"));
		assert!(result.contains("/b.rs"));
		assert!(result.contains("/c.rs"));
	}

	#[test]
	fn test_cwd_filters_files() {
		let fixture = Fixture::parse(
			r#"
//- /data/app/file.txt
app content
//- /data/other/file.txt
other content
//- /cache/temp.txt
temp
"#,
		);
		let temp = fixture.write_to_tempdir().cwd("data/app");

		let result = temp.read_all_from_disk();
		assert_eq!(result.files.len(), 1);
		assert_eq!(result.files[0].path, "/file.txt");
		assert!(result.files[0].text.contains("app content"));
	}

	#[test]
	fn test_cwd_with_leading_slash() {
		let fixture = Fixture::parse(
			r#"
//- /src/main.rs
fn main() {}
//- /tests/test.rs
fn test() {}
"#,
		);
		// Both "/src" and "src" should work the same
		let temp = fixture.write_to_tempdir().cwd("/src");

		let result = temp.read_all_from_disk();
		assert_eq!(result.files.len(), 1);
		assert_eq!(result.files[0].path, "/main.rs");
	}

	#[test]
	fn test_cwd_nested_paths() {
		let fixture = Fixture::parse(
			r#"
//- /data/todo/blockers/work.md
- task 1
//- /data/todo/blockers/home.md
- task 2
//- /data/todo/config.json
{}
"#,
		);
		let temp = fixture.write_to_tempdir().cwd("data/todo");

		let result = temp.read_all_from_disk();
		assert_eq!(result.files.len(), 3);
		assert!(result.contains("/blockers/work.md"));
		assert!(result.contains("/blockers/home.md"));
		assert!(result.contains("/config.json"));
	}

	#[test]
	fn test_no_cwd_includes_all() {
		let fixture = Fixture::parse(
			r#"
//- /data/file.txt
data
//- /cache/file.txt
cache
"#,
		);
		let temp = fixture.write_to_tempdir();

		let result = temp.read_all_from_disk();
		assert_eq!(result.files.len(), 2);
		assert!(result.contains("/data/file.txt"));
		assert!(result.contains("/cache/file.txt"));
	}

	#[test]
	fn test_read_from_directory() {
		// Create a temp directory with some files
		let fixture = Fixture::parse(
			r#"
//- /src/main.rs
fn main() {}
//- /src/lib.rs
pub fn lib() {}
//- /README.md
# Test
"#,
		);
		let temp = fixture.write_to_tempdir();

		// Read it back using read_from_directory
		let result = Fixture::read_from_directory(temp.root.clone()).unwrap();
		assert_eq!(result.files.len(), 3);
		assert!(result.contains("/src/main.rs"));
		assert!(result.contains("/src/lib.rs"));
		assert!(result.contains("/README.md"));
	}

	#[test]
	fn test_read_from_directory_nonexistent() {
		let result = Fixture::read_from_directory("/nonexistent/path/that/does/not/exist");
		assert!(result.is_none());
	}

	#[test]
	fn test_read_from_directory_skips_git() {
		let fixture = Fixture::parse(
			r#"
//- /file.txt
content
//- /.git/config
git config
//- /.git/HEAD
ref: refs/heads/main
"#,
		);
		let temp = fixture.write_to_tempdir();

		let result = Fixture::read_from_directory(temp.root.clone()).unwrap();
		// Should only have file.txt, not .git contents
		assert_eq!(result.files.len(), 1);
		assert!(result.contains("/file.txt"));
	}

	#[test]
	fn test_fixture_renderer_redact_lines() {
		// Multi-file fixture so we get the //- header lines
		let fixture = Fixture::parse(
			r#"
//- /config.json
{
  "name": "test",
  "timestamp": "2026-01-22T12:00:00Z",
  "value": 42
}
//- /other.txt
other
"#,
		);

		// Rendered output:
		// 1: //- /config.json
		// 2: {
		// 3:   "name": "test",
		// 4:   "timestamp": "2026-01-22T12:00:00Z",
		// 5:   "value": 42
		// 6: }
		// 7: //- /other.txt
		// 8: other
		let rendered = FixtureRenderer::new(&fixture).redact_lines(&[4]).render();

		assert!(rendered.contains("\"name\": \"test\""));
		assert!(rendered.contains("[REDACTED]"));
		assert!(!rendered.contains("2026-01-22"));
		assert!(rendered.contains("\"value\": 42"));
	}

	#[test]
	fn test_fixture_renderer_custom_redact_message() {
		let fixture = Fixture::parse(
			r#"
line 1
line 2
line 3
"#,
		);

		let rendered = FixtureRenderer::new(&fixture).redact_lines(&[2]).redact_message("[TIMESTAMP REDACTED]").render();

		assert!(rendered.contains("line 1"));
		assert!(rendered.contains("[TIMESTAMP REDACTED]"));
		assert!(rendered.contains("line 3"));
		assert!(!rendered.contains("line 2"));
	}

	#[test]
	fn test_fixture_renderer_normalize_git_hashes() {
		let fixture = Fixture::parse(
			r#"
<<<<<<< HEAD
local content
||||||| a0f7d74
original content
=======
remote content
>>>>>>> feature
"#,
		);

		let rendered = FixtureRenderer::new(&fixture).normalize_git_hashes().render();

		assert!(rendered.contains("||||||| [hash]"));
		assert!(!rendered.contains("a0f7d74"));
	}

	#[test]
	fn test_fixture_renderer_combined() {
		// Multi-file fixture
		let fixture = Fixture::parse(
			r#"
//- /file.txt
line 1
timestamp: 2026-01-22T12:00:00Z
||||||| abc1234
conflict marker
//- /other.txt
other
"#,
		);

		// Rendered output:
		// 1: //- /file.txt
		// 2: line 1
		// 3: timestamp: 2026-01-22T12:00:00Z
		// 4: ||||||| abc1234
		// 5: conflict marker
		// 6: //- /other.txt
		// 7: other
		let rendered = FixtureRenderer::new(&fixture)
			.normalize_git_hashes()
			.redact_lines(&[3]) // "timestamp: ..." line in rendered output
			.render();

		assert!(rendered.contains("line 1"));
		assert!(rendered.contains("[REDACTED]"));
		assert!(rendered.contains("||||||| [hash]"));
		assert!(!rendered.contains("abc1234"));
	}

	#[test]
	fn test_fixture_renderer_regex_inclusion() {
		let fixture = Fixture::parse(
			r#"
//- /src/main.rs
fn main() {}
//- /src/lib.rs
pub fn lib() {}
//- /tests/integration.rs
fn test_integration() {}
//- /benches/bench.rs
fn benchmark() {}
"#,
		);

		// Include only files under /src/
		let rendered = FixtureRenderer::new(&fixture).regex("^/src/").render();

		assert!(rendered.contains("//- /src/main.rs"));
		assert!(rendered.contains("//- /src/lib.rs"));
		assert!(!rendered.contains("/tests/"));
		assert!(!rendered.contains("/benches/"));
	}

	#[test]
	fn test_fixture_renderer_regex_exclusion() {
		let fixture = Fixture::parse(
			r#"
//- /src/main.rs
fn main() {}
//- /src/lib.rs
pub fn lib() {}
//- /tests/integration.rs
fn test_integration() {}
//- /tests/unit.rs
fn test_unit() {}
"#,
		);

		// Exclude all test files
		let rendered = FixtureRenderer::new(&fixture).regex("!/tests/").render();

		assert!(rendered.contains("//- /src/main.rs"));
		assert!(rendered.contains("//- /src/lib.rs"));
		assert!(!rendered.contains("/tests/"));
		assert!(!rendered.contains("integration"));
		assert!(!rendered.contains("unit"));
	}
}
