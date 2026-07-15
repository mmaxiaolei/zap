pub mod domain;
pub mod git;
pub mod migration;
pub mod model;
pub mod view;

#[cfg(test)]
#[path = "git_tests.rs"]
mod git_tests;

#[cfg(test)]
#[path = "migration_tests.rs"]
mod migration_tests;
