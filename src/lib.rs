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
//! use v_fixtures::{Fixture, render_fixture};
//!
//! // Apply some transformation to files...
//! let temp = fixture.write_to_tempdir();
//! // ... run your tool ...
//! let result = temp.read_all_from_disk();
//! insta::assert_snapshot!(render_fixture(&result), @"...");
//! ```

use std::{fs, path::PathBuf};

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
}

impl TempFixture {
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
	/// Returns files sorted by path for deterministic output
	pub fn read_all_from_disk(&self) -> Fixture {
		let mut files: Vec<FixtureFile> = Vec::new();

		for entry in walkdir::WalkDir::new(&self.root).into_iter().filter_map(Result::ok) {
			let path = entry.path();
			if path.is_file() {
				let relative_path = path.strip_prefix(&self.root).expect("path should be under root");
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

/// Render a fixture back to string format (for snapshots)
///
/// Single-file fixtures render as just the content.
/// Multi-file fixtures render with `//- /path` markers.
pub fn render_fixture(fixture: &Fixture) -> String {
	if fixture.files.len() == 1 {
		return fixture.files[0].text.clone();
	}

	let mut result = String::new();
	for file in &fixture.files {
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
	fn test_render_fixture_single() {
		let fixture = Fixture {
			files: vec![FixtureFile {
				path: "/main.rs".to_owned(),
				text: "fn main() {}\n".to_owned(),
			}],
		};
		let rendered = render_fixture(&fixture);
		assert_eq!(rendered, "fn main() {}\n");
	}

	#[test]
	fn test_render_fixture_multi() {
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
		let rendered = render_fixture(&fixture);
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
}
