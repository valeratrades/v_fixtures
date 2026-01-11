//! Git repository wrapper for test fixtures.
//!
//! Provides utilities for creating and manipulating git repositories in tests.
//!
//! # Example
//!
//! ```
//! use v_fixtures::{Fixture, fs_standards::git::Git};
//!
//! let fixture = Fixture::parse(r#"
//! //- /README.md
//! # Test Project
//! "#);
//! let git = Git::init(fixture.write_to_tempdir());
//!
//! // Make an initial commit
//! git.add_all();
//! git.commit("Initial commit");
//!
//! // Check status
//! assert!(git.is_clean());
//! ```

use std::{
	path::PathBuf,
	process::{Command, Output},
};

use crate::TempFixture;

/// Git repository wrapper for test fixtures.
///
/// Initializes a git repository in the fixture root and provides
/// helper methods for common git operations.
pub struct Git {
	/// The underlying TempFixture
	pub inner: TempFixture,
}

impl Git {
	/// Initialize a new git repository in the fixture root.
	///
	/// Also configures a test user (name and email) for commits.
	pub fn init(inner: TempFixture) -> Self {
		let git = Self { inner };
		git.run(&["init"]).expect("git init failed");
		git.run(&["config", "user.email", "test@test.local"]).expect("git config email failed");
		git.run(&["config", "user.name", "Test User"]).expect("git config name failed");
		git
	}

	/// Initialize git in a specific subdirectory of the fixture.
	///
	/// Useful when the git repo should be inside the fixture rather than at root.
	pub fn init_in(inner: TempFixture, subdir: &str) -> Self {
		let git = Self { inner };
		let dir = git.inner.root.join(subdir);
		std::fs::create_dir_all(&dir).expect("failed to create git directory");
		git.run_in(&dir, &["init"]).expect("git init failed");
		git.run_in(&dir, &["config", "user.email", "test@test.local"]).expect("git config email failed");
		git.run_in(&dir, &["config", "user.name", "Test User"]).expect("git config name failed");
		git
	}

	/// Get the root directory of the fixture.
	pub fn root(&self) -> &PathBuf {
		&self.inner.root
	}

	/// Run a git command in the repository root.
	pub fn run(&self, args: &[&str]) -> std::io::Result<Output> {
		Command::new("git").args(args).current_dir(&self.inner.root).output()
	}

	/// Run a git command in a specific directory.
	pub fn run_in(&self, dir: &PathBuf, args: &[&str]) -> std::io::Result<Output> {
		Command::new("git").args(args).current_dir(dir).output()
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
		self.inner.root.join(".git/MERGE_HEAD").exists()
	}

	/// Get list of files with conflicts.
	pub fn conflicted_files(&self) -> Vec<String> {
		let output = self.run(&["diff", "--name-only", "--diff-filter=U"]).expect("git diff failed");
		String::from_utf8_lossy(&output.stdout).lines().map(|s| s.to_string()).collect()
	}

	/// Read a file from the repository.
	pub fn read(&self, path: &str) -> String {
		std::fs::read_to_string(self.inner.root.join(path)).expect("failed to read file")
	}

	/// Write a file to the repository.
	pub fn write(&self, path: &str, content: &str) {
		let full_path = self.inner.root.join(path);
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
		let git = Git::init(fixture.write_to_tempdir());

		// Should have .git directory
		assert!(git.inner.root.join(".git").exists());
	}

	#[test]
	fn test_git_commit() {
		let fixture = Fixture::parse(
			r#"
//- /file.txt
content
"#,
		);
		let git = Git::init(fixture.write_to_tempdir());

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
		let git = Git::init(fixture.write_to_tempdir());

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
		let git = Git::init(fixture.write_to_tempdir());

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
}
