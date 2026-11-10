// SPDX-License-Identifier: GPL-3.0-or-later
//
// A project's catalog (docs/library-management.md §9): what's in its folder and its locations — every file and
// folder, and for the files whose insides count (moments, clip previews, exports, photos: their link; clips' sidecars:
// marks and tags) what's inside them — shared between computers through the project's own folder, .afp/catalog/.
//
// The files are the truth; the catalog is what was last seen of them, kept so that nothing has to look at every file
// again: each computer appends what it sees and does to its own journal, a snapshot now and then folds the journals,
// and a reader reads the newest snapshot (only the parts it needs) and the journals' lines after it. Nothing is ever
// written over, no lock is taken on the share, no clock is compared. Lost, it's built again from the files.

pub mod catalog;
pub mod cli;
pub mod inspect;
pub mod locations;
pub mod project;
pub mod journal;
pub mod sim;
pub mod snapshot;
pub mod state;
pub mod store;

pub use catalog::{ computer_id, machine_id, Catalog, Error, Inspect, Loaded, Report, Root, Sweep, Want };
pub use state::{ File, State };
pub use store::{ DiskStore, Store };
