//! XDG Base Directory Specification layout.
//!
//! Wraps a TempFixture to provide XDG-style directory access with separate
//! data, state, cache, config, and runtime directories, organized around an app name.
//!
//! # Example
//!
//! ```
//! use v_fixtures::{Fixture, fs_standards::xdg::Xdg};
//!
//! // Create base fixture and wrap with XDG layout for "myapp"
//! let fixture = Fixture::parse(r#"
//! //- /data/notes.md
//! # Notes
//! //- /cache/temp.txt
//! cached data
//! "#);
//! let xdg = Xdg::new(fixture.write_to_tempdir(), "myapp");
//!
//! // Files are accessible under {xdg_dir}/{app_name}/...
//! assert!(xdg.data_dir().join("notes.md").exists());
//! assert!(xdg.cache_dir().join("temp.txt").exists());
//!
//! // env_vars() returns XDG env vars pointing to parent dirs
//! // so apps using xdg_data_dir!("subpath") get {root}/data/{app}/subpath
//! for (key, value) in xdg.env_vars() {
//!     // e.g., XDG_DATA_HOME -> {root}/data
//!     println!("{key}={}", value.display());
//! }
//! ```

use std::{fs, path::PathBuf};

use crate::TempFixture;

/// XDG Base Directory layout wrapper.
///
/// Organizes files around an app name following XDG conventions:
/// - Fixture path `/data/file.txt` → `{root}/data/{app_name}/file.txt`
/// - Fixture path `/state/file.txt` → `{root}/state/{app_name}/file.txt`
/// - etc.
///
/// The `env_vars()` method returns environment variables that point to the
/// parent directories (e.g., `XDG_DATA_HOME={root}/data`), so apps using
/// `v_utils::xdg_data_dir!("subpath")` will correctly resolve to
/// `{root}/data/{app_name}/subpath`.
pub struct Xdg {
	/// The underlying TempFixture
	pub inner: TempFixture,
	/// The app name used for subdirectories
	pub app_name: String,
}

impl Xdg {
	/// Wrap a TempFixture with XDG directory accessors.
	///
	/// Files in the fixture should be organized under `/data/`, `/state/`, `/cache/`, etc.
	/// These will be moved to `{root}/data/{app_name}/`, `{root}/state/{app_name}/`, etc.
	///
	/// # Arguments
	/// * `inner` - The TempFixture containing files organized by XDG category
	/// * `app_name` - The application name to use for subdirectories
	pub fn new(inner: TempFixture, app_name: &str) -> Self {
		let app_name = app_name.to_string();
		let xdg_dirs = ["data", "state", "cache", "config", "runtime"];

		// For each XDG directory, move contents from {root}/{dir}/* to {root}/{dir}/{app_name}/*
		for dir in xdg_dirs {
			let category_dir = inner.root.join(dir);
			let app_dir = category_dir.join(&app_name);

			if category_dir.exists() {
				// Create temp dir, move contents there, then move back under app_name
				let temp_dir = inner.root.join(format!(".{dir}_temp"));
				if let Ok(entries) = fs::read_dir(&category_dir) {
					// Move existing contents to temp
					fs::create_dir_all(&temp_dir).expect("failed to create temp dir");
					for entry in entries.flatten() {
						let from = entry.path();
						let to = temp_dir.join(entry.file_name());
						fs::rename(&from, &to).expect("failed to move to temp");
					}
					// Create app subdir and move contents back
					fs::create_dir_all(&app_dir).expect("failed to create app dir");
					if let Ok(temp_entries) = fs::read_dir(&temp_dir) {
						for entry in temp_entries.flatten() {
							let from = entry.path();
							let to = app_dir.join(entry.file_name());
							fs::rename(&from, &to).expect("failed to move from temp");
						}
					}
					fs::remove_dir_all(&temp_dir).ok();
				}
			} else {
				// Just create the app directory
				fs::create_dir_all(&app_dir).expect("failed to create XDG app directory");
			}
		}

		Self { inner, app_name }
	}

	/// Get the data directory path (`{root}/data/{app_name}`).
	pub fn data_dir(&self) -> PathBuf {
		self.inner.root.join("data").join(&self.app_name)
	}

	/// Get the state directory path (`{root}/state/{app_name}`).
	pub fn state_dir(&self) -> PathBuf {
		self.inner.root.join("state").join(&self.app_name)
	}

	/// Get the cache directory path (`{root}/cache/{app_name}`).
	pub fn cache_dir(&self) -> PathBuf {
		self.inner.root.join("cache").join(&self.app_name)
	}

	/// Get the config directory path (`{root}/config/{app_name}`).
	pub fn config_dir(&self) -> PathBuf {
		self.inner.root.join("config").join(&self.app_name)
	}

	/// Get the runtime directory path (`{root}/runtime/{app_name}`).
	pub fn runtime_dir(&self) -> PathBuf {
		self.inner.root.join("runtime").join(&self.app_name)
	}

	/// Read a file from the data directory.
	pub fn read_data(&self, relative: &str) -> String {
		fs::read_to_string(self.data_dir().join(relative)).expect("failed to read file")
	}

	/// Read a file from the state directory.
	pub fn read_state(&self, relative: &str) -> String {
		fs::read_to_string(self.state_dir().join(relative)).expect("failed to read file")
	}

	/// Read a file from the cache directory.
	pub fn read_cache(&self, relative: &str) -> String {
		fs::read_to_string(self.cache_dir().join(relative)).expect("failed to read file")
	}

	/// Read a file from the config directory.
	pub fn read_config(&self, relative: &str) -> String {
		fs::read_to_string(self.config_dir().join(relative)).expect("failed to read file")
	}

	/// Check if a file exists in the data directory.
	pub fn data_exists(&self, relative: &str) -> bool {
		self.data_dir().join(relative).exists()
	}

	/// Check if a file exists in the state directory.
	pub fn state_exists(&self, relative: &str) -> bool {
		self.state_dir().join(relative).exists()
	}

	/// Check if a file exists in the cache directory.
	pub fn cache_exists(&self, relative: &str) -> bool {
		self.cache_dir().join(relative).exists()
	}

	/// Check if a file exists in the config directory.
	pub fn config_exists(&self, relative: &str) -> bool {
		self.config_dir().join(relative).exists()
	}

	/// Write a file to the data directory.
	pub fn write_data(&self, relative: &str, content: &str) {
		let path = self.data_dir().join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		fs::write(path, content).expect("failed to write file");
	}

	/// Write a file to the state directory.
	pub fn write_state(&self, relative: &str, content: &str) {
		let path = self.state_dir().join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		fs::write(path, content).expect("failed to write file");
	}

	/// Write a file to the cache directory.
	pub fn write_cache(&self, relative: &str, content: &str) {
		let path = self.cache_dir().join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		fs::write(path, content).expect("failed to write file");
	}

	/// Write a file to the config directory.
	pub fn write_config(&self, relative: &str, content: &str) {
		let path = self.config_dir().join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		fs::write(path, content).expect("failed to write file");
	}

	/// Get environment variables for running a subprocess that uses XDG directories.
	///
	/// Returns (key, value) pairs for XDG_DATA_HOME, XDG_STATE_HOME, XDG_CACHE_HOME,
	/// XDG_CONFIG_HOME, and XDG_RUNTIME_DIR.
	///
	/// Each variable points to the category directory (e.g., `{root}/data`), so apps
	/// using `v_utils::xdg_data_dir!("subpath")` will resolve to
	/// `{root}/data/{app_name}/subpath`.
	///
	/// # Example
	///
	/// ```ignore
	/// use std::process::Command;
	/// use v_fixtures::{Fixture, fs_standards::xdg::Xdg};
	///
	/// let xdg = Xdg::new(Fixture::parse("...").write_to_tempdir(), "myapp");
	/// let mut cmd = Command::new("my-app");
	/// for (key, value) in xdg.env_vars() {
	///     cmd.env(key, value);
	/// }
	/// ```
	pub fn env_vars(&self) -> Vec<(&'static str, PathBuf)> {
		vec![
			("XDG_DATA_HOME", self.inner.root.join("data")),
			("XDG_STATE_HOME", self.inner.root.join("state")),
			("XDG_CACHE_HOME", self.inner.root.join("cache")),
			("XDG_CONFIG_HOME", self.inner.root.join("config")),
			("XDG_RUNTIME_DIR", self.inner.root.join("runtime")),
		]
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Fixture;

	#[test]
	fn test_xdg_layout() {
		let fixture = Fixture::parse(
			r#"
//- /data/blockers/test.md
# Project
- task 1
//- /cache/current.txt
project.md
//- /state/state.json
{}
"#,
		);
		let xdg = Xdg::new(fixture.write_to_tempdir(), "testapp");

		assert!(xdg.data_exists("blockers/test.md"));
		assert!(xdg.cache_exists("current.txt"));
		assert!(xdg.state_exists("state.json"));
		assert!(xdg.read_data("blockers/test.md").contains("# Project"));
	}

	#[test]
	fn test_xdg_write() {
		let fixture = Fixture::parse("");
		let xdg = Xdg::new(fixture.write_to_tempdir(), "testapp");

		xdg.write_data("new/file.txt", "hello");
		assert!(xdg.data_exists("new/file.txt"));
		assert_eq!(xdg.read_data("new/file.txt"), "hello");
	}

	#[test]
	fn test_env_vars() {
		let fixture = Fixture::parse("");
		let xdg = Xdg::new(fixture.write_to_tempdir(), "myapp");

		let env_vars = xdg.env_vars();
		assert_eq!(env_vars.len(), 5);

		// Check that XDG_DATA_HOME points to {root}/data (parent of app dir)
		let data_home = env_vars.iter().find(|(k, _)| *k == "XDG_DATA_HOME").unwrap();
		assert_eq!(data_home.1, xdg.inner.root.join("data"));

		// The app's data dir should be XDG_DATA_HOME/myapp
		assert_eq!(xdg.data_dir(), data_home.1.join("myapp"));
	}

	#[test]
	fn test_directory_structure() {
		let fixture = Fixture::parse(
			r#"
//- /data/file.txt
data content
//- /state/file.txt
state content
"#,
		);
		let xdg = Xdg::new(fixture.write_to_tempdir(), "myapp");

		// Files should be at {root}/{category}/myapp/file.txt
		assert!(xdg.inner.root.join("data/myapp/file.txt").exists());
		assert!(xdg.inner.root.join("state/myapp/file.txt").exists());

		// And accessible via the helper methods
		assert!(xdg.read_data("file.txt").contains("data content"));
		assert!(xdg.read_state("file.txt").contains("state content"));
	}
}
