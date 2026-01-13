//! Git repository wrapper for test fixtures.
//!
//! Provides utilities for creating and manipulating git repositories in tests.
//!
//! # Basic usage with a fixture
//!
//! ```
//! use v_fixtures::{Fixture, fs_standards::git::Git};
//!
//! let fixture = Fixture::parse(r#"
//! //- /README.md
//! # Test Project
//! "#);
//! let temp = fixture.write_to_tempdir();
//! let git = Git::init(&temp.root);
//!
//! // Make an initial commit
//! git.add_all();
//! git.commit("Initial commit");
//!
//! // Check status
//! assert!(git.is_clean());
//! ```
//!
//! # Usage with any path
//!
//! ```ignore
//! use v_fixtures::fs_standards::git::Git;
//!
//! // Initialize git in any directory
//! let git = Git::init("/tmp/my-repo");
//! git.add_all();
//! git.commit("Initial");
//! ```

use std::{
	path::{Path, PathBuf},
	process::{Command, Output},
};

/// Git repository wrapper for any directory.
///
/// Provides helper methods for common git operations in tests.
/// Does not own the directory - just wraps a path.
pub struct Git {
	/// Root path of the git repository
	pub root: PathBuf,
}

impl Git {
	/// Initialize a new git repository at the given path.
	///
	/// Creates the directory if it doesn't exist, runs `git init`,
	/// and configures a test user (name and email) for commits.
	pub fn init(root: impl Into<PathBuf>) -> Self {
		let root = root.into();
		std::fs::create_dir_all(&root).expect("failed to create git directory");

		let git = Self { root };
		git.run(&["init"]).expect("git init failed");
		git.run(&["config", "user.email", "test@test.local"]).expect("git config email failed");
		git.run(&["config", "user.name", "Test User"]).expect("git config name failed");
		git
	}

	/// Wrap an existing git repository at the given path.
	///
	/// Does not run `git init` - assumes the repository already exists.
	pub fn open(root: impl Into<PathBuf>) -> Self {
		Self { root: root.into() }
	}

	/// Run a git command in the repository.
	pub fn run(&self, args: &[&str]) -> std::io::Result<Output> {
		Command::new("git").args(args).current_dir(&self.root).output()
	}

	/// Stage all changes.
	pub fn add_all(&self) {
		self.run(&["add", "-A"]).expect("git add failed");
	}

	/// Stage specific files.
	pub fn add(&self, paths: &[&str]) {
		let mut args = vec!["add"];
		args.extend(paths);
		self.run(&args).expect("git add failed");
	}

	/// Create a commit with the given message.
	///
	/// Returns the commit hash.
	pub fn commit(&self, message: &str) -> String {
		let output = self.run(&["commit", "-m", message]).expect("git commit failed");
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			panic!("git commit failed: {stderr}");
		}
		self.head_hash()
	}

	/// Get the current HEAD commit hash.
	pub fn head_hash(&self) -> String {
		let output = self.run(&["rev-parse", "HEAD"]).expect("git rev-parse failed");
		String::from_utf8_lossy(&output.stdout).trim().to_string()
	}

	/// Get the current branch name.
	pub fn current_branch(&self) -> String {
		let output = self.run(&["rev-parse", "--abbrev-ref", "HEAD"]).expect("git rev-parse failed");
		String::from_utf8_lossy(&output.stdout).trim().to_string()
	}

	/// Check if the working tree is clean (no uncommitted changes).
	pub fn is_clean(&self) -> bool {
		let output = self.run(&["status", "--porcelain"]).expect("git status failed");
		output.stdout.is_empty()
	}

	/// Get the status output.
	pub fn status(&self) -> String {
		let output = self.run(&["status"]).expect("git status failed");
		String::from_utf8_lossy(&output.stdout).to_string()
	}

	/// Create a new branch.
	pub fn create_branch(&self, name: &str) {
		self.run(&["branch", name]).expect("git branch failed");
	}

	/// Checkout a branch.
	pub fn checkout(&self, name: &str) {
		let output = self.run(&["checkout", name]).expect("git checkout failed");
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			panic!("git checkout failed: {stderr}");
		}
	}

	/// Create and checkout a new branch.
	pub fn checkout_new_branch(&self, name: &str) {
		let output = self.run(&["checkout", "-b", name]).expect("git checkout -b failed");
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			panic!("git checkout -b failed: {stderr}");
		}
	}

	/// Merge a branch into current branch.
	///
	/// Returns Ok(()) if merge succeeded, Err with conflict info if conflicts.
	pub fn merge(&self, branch: &str) -> Result<(), String> {
		let output = self.run(&["merge", branch, "-m", &format!("Merge {branch}")]).expect("git merge failed");
		if output.status.success() {
			Ok(())
		} else {
			let stdout = String::from_utf8_lossy(&output.stdout);
			let stderr = String::from_utf8_lossy(&output.stderr);
			Err(format!("{stdout}\n{stderr}"))
		}
	}

	/// Abort an in-progress merge.
	pub fn merge_abort(&self) {
		self.run(&["merge", "--abort"]).ok();
	}

	/// Delete a branch.
	pub fn delete_branch(&self, name: &str) {
		self.run(&["branch", "-D", name]).ok();
	}

	/// Check if currently in a merge conflict state.
	pub fn has_conflicts(&self) -> bool {
		self.root.join(".git/MERGE_HEAD").exists()
	}

	/// Get list of files with conflicts.
	pub fn conflicted_files(&self) -> Vec<String> {
		let output = self.run(&["diff", "--name-only", "--diff-filter=U"]).expect("git diff failed");
		String::from_utf8_lossy(&output.stdout).lines().map(|s| s.to_string()).collect()
	}

	/// Read a file from the repository.
	pub fn read(&self, path: impl AsRef<Path>) -> String {
		std::fs::read_to_string(self.root.join(path)).expect("failed to read file")
	}

	/// Write a file to the repository.
	pub fn write(&self, path: impl AsRef<Path>, content: &str) {
		let full_path = self.root.join(path);
		if let Some(parent) = full_path.parent() {
			std::fs::create_dir_all(parent).expect("failed to create parent dirs");
		}
		std::fs::write(full_path, content).expect("failed to write file");
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Fixture;

	#[test]
	fn test_git_init() {
		let fixture = Fixture::parse(
			r#"
//- /README.md
# Test
"#,
		);
		let temp = fixture.write_to_tempdir();
		let git = Git::init(&temp.root);

		// Should have .git directory
		assert!(temp.root.join(".git").exists());
		assert!(git.is_clean() || !git.is_clean()); // just check it doesn't panic
	}

	#[test]
	fn test_git_commit() {
		let fixture = Fixture::parse(
			r#"
//- /file.txt
content
"#,
		);
		let temp = fixture.write_to_tempdir();
		let git = Git::init(&temp.root);

		git.add_all();
		let hash = git.commit("Initial commit");

		assert!(!hash.is_empty());
		assert!(git.is_clean());
	}

	#[test]
	fn test_git_branch_and_merge() {
		let fixture = Fixture::parse(
			r#"
//- /file.txt
original
"#,
		);
		let temp = fixture.write_to_tempdir();
		let git = Git::init(&temp.root);

		git.add_all();
		git.commit("Initial");

		// Create a branch and make changes
		git.checkout_new_branch("feature");
		git.write("file.txt", "feature content");
		git.add_all();
		git.commit("Feature change");

		// Go back to master and merge
		git.checkout("master");
		assert!(git.merge("feature").is_ok());

		assert_eq!(git.read("file.txt"), "feature content");
	}

	#[test]
	fn test_git_conflict_detection() {
		let fixture = Fixture::parse(
			r#"
//- /file.txt
original
"#,
		);
		let temp = fixture.write_to_tempdir();
		let git = Git::init(&temp.root);

		git.add_all();
		git.commit("Initial");

		// Create a branch and make changes
		git.checkout_new_branch("feature");
		git.write("file.txt", "feature version");
		git.add_all();
		git.commit("Feature");

		// Go back to master and make conflicting changes
		git.checkout("master");
		git.write("file.txt", "master version");
		git.add_all();
		git.commit("Master");

		// Merge should fail with conflict
		let result = git.merge("feature");
		assert!(result.is_err());
		assert!(git.has_conflicts());
		assert!(git.conflicted_files().contains(&"file.txt".to_string()));

		// Cleanup
		git.merge_abort();
	}

	#[test]
	fn test_git_init_in_subdir() {
		let fixture = Fixture::parse("");
		let temp = fixture.write_to_tempdir();

		// Initialize git in a subdirectory
		let subdir = temp.root.join("nested/repo");
		let git = Git::init(&subdir);

		assert!(subdir.join(".git").exists());
		git.write("test.txt", "hello");
		git.add_all();
		git.commit("Test commit");
		assert!(git.is_clean());
	}
}
