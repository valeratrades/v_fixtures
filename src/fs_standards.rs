//! Filesystem standard extensions for TempFixture.
//!
//! Provides wrappers around TempFixture that understand different filesystem
//! layout standards (XDG, FHS, etc.) and common tools like git.

pub mod git;
pub mod xdg;

pub mod fhs {
	//! FHS (Filesystem Hierarchy Standard) layout.
	//!
	//! Standard Unix layout with /etc, /var, /usr, etc.

	use crate::TempFixture;

	/// FHS filesystem layout wrapper.
	pub struct Fhs {
		pub inner: TempFixture,
	}

	impl Fhs {
		/// Create a new FHS fixture from a TempFixture.
		pub fn new(_inner: TempFixture) -> Self {
			todo!("FHS layout not yet implemented")
		}
	}
}
