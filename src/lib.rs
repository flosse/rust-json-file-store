// Copyright (c) 2016 - 2019 Markus Kohlhase <mail@markus-kohlhase.de>

//! A simple JSON file store written in Rust.
//! This is a port and drop-in replacement of the Node.js library
//! [json-file-store](https://github.com/flosse/json-file-store/).
//!
//! **WARNING**:
//! Don't use it if you want to persist a large amount of objects.
//! Use a real DB instead.
//!
//! # Example
//!
//! ```rust,no_run
//! extern crate jfs;
//! #[macro_use]
//! extern crate serde_derive;
//! use jfs::Store;
//!
//! #[derive(Serialize,Deserialize)]
//! struct Foo {
//!     foo: String
//! }
//!
//! pub fn main() {
//!     let db = Store::new("data").unwrap();
//!     let f = Foo { foo: "bar".to_owned() };
//!     let id = db.save(&f).unwrap();
//!     let obj = db.get::<Foo>(&id).unwrap();
//!     db.delete(&id).unwrap();
//! }
//! ```
//!
//! You can also store all data in one single JSON-File:
//!
//! ```rust,no_run
//! let mut cfg = jfs::Config::default();
//! cfg.single = true; // false is default
//! let db = jfs::Store::new_with_cfg("data",cfg);
//! ```
//!
//! If you like to pretty print the file content, set `pretty` to `true`
//! and choose a number of whitespaces for the indention:
//!
//! ```rust,no_run
//! let mut cfg = jfs::Config::default();
//! cfg.pretty = true;  // false is default
//! cfg.indent = 4;     // 2 is default
//! ```
//!
//! Creating a store instance that is living in the memory can be done like this:
//!
//! ```rust,no_run
//! let db = jfs::Store::new(jfs::IN_MEMORY).unwrap();
//! ```

use log::error;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Result,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

mod file_store;
mod json_store;
mod memory_store;

use file_store::FileStore;
use json_store::JsonStore;
use memory_store::MemoryStore;

pub use file_store::Config;

#[derive(Clone)]
pub struct Store(StoreType);

#[derive(Clone)]
enum StoreType {
    File(Arc<RwLock<FileStore>>, PathBuf),
    Memory(MemoryStore),
}

pub const IN_MEMORY: &str = "::memory::";

impl Store {
    /// Opens a `Store` against the specified path.
    ///
    /// See `new_with_cfg(..)` for more details
    ///
    /// # Arguments
    ///
    /// * `path` - path to the db directory of JSON documents
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Store::new_with_cfg(path, Config::default())
    }

    /// Opens a `Store` against the specified path with the given configuration
    ///
    /// If the `Store` already exists, it will be opened, otherwise this has the side-effect of creating the new `Store`
    ///  and the backing directories and files.
    ///
    /// # Arguments
    ///
    /// * `path` - path to the db directory of JSON documents, if configured for single db mode then `.json` will be used as the extension (replacing any existing extension)
    /// * `cfg` - configuration for the DB instance
    pub fn new_with_cfg<P: AsRef<Path>>(path: P, cfg: Config) -> Result<Self> {
        if path.as_ref() == Path::new(IN_MEMORY) {
            Ok(Self(StoreType::Memory(MemoryStore::default())))
        } else {
            let s = FileStore::new_with_cfg(path, cfg)?;
            let p = s.path().to_path_buf();
            Ok(Self(StoreType::File(Arc::new(RwLock::new(s)), p)))
        }
    }

    /// Returns the storage path for the backing JSON store.
    ///
    /// In single-file-mode this will be the JSON file location, otherwise it's
    ///  the directory in which all JSON objects are stored.
    pub fn path(&self) -> &Path {
        match &self.0 {
            StoreType::File(_, p) => p,
            StoreType::Memory(_) => Path::new(IN_MEMORY),
        }
    }

    pub fn save<T>(&self, obj: &T) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        match &self.0 {
            StoreType::File(f, _) => f.write().unwrap_or_else(handle_write_err).save(obj),
            StoreType::Memory(m) => m.save(obj),
        }
    }

    pub fn save_with_id<T>(&self, obj: &T, id: &str) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        match &self.0 {
            StoreType::File(f, _) => f
                .write()
                .unwrap_or_else(handle_write_err)
                .save_with_id(obj, id),
            StoreType::Memory(m) => m.save_with_id(obj, id),
        }
    }

    pub fn get<T>(&self, id: &str) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        match &self.0 {
            StoreType::File(f, _) => f.read().unwrap_or_else(handle_read_err).get(id),
            StoreType::Memory(m) => m.get(id),
        }
    }

    pub fn all<T>(&self) -> Result<BTreeMap<String, T>>
    where
        for<'de> T: Deserialize<'de>,
    {
        match &self.0 {
            StoreType::File(f, _) => f.read().unwrap_or_else(handle_read_err).all(),
            StoreType::Memory(m) => m.all(),
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        match &self.0 {
            StoreType::File(f, _) => f.write().unwrap_or_else(handle_write_err).delete(id),
            StoreType::Memory(m) => m.delete(id),
        }
    }
}

fn handle_write_err<'a, T>(err: PoisonError<RwLockWriteGuard<'a, T>>) -> RwLockWriteGuard<'a, T> {
    error!("Write lock poisoned");
    err.into_inner()
}

fn handle_read_err<'a, T>(err: PoisonError<RwLockReadGuard<'a, T>>) -> RwLockReadGuard<'a, T> {
    error!("Read lock poisoned");
    err.into_inner()
}

#[cfg(test)]
mod tests {

    use super::*;
    use serde_derive::{Deserialize, Serialize};
    use std::thread;
    use tempdir::TempDir;

    #[derive(Serialize, Deserialize)]
    struct Data {
        x: i32,
    }

    fn multi_threaded_write(store: Store) {
        let mut threads: Vec<thread::JoinHandle<()>> = vec![];
        for i in 0..20 {
            let db = store.clone();
            let x = Data { x: i };
            threads.push(thread::spawn(move || {
                db.save_with_id(&x, &i.to_string()).unwrap();
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
        let all = store.all::<Data>().unwrap();
        assert_eq!(all.len(), 20);
        for (id, data) in all {
            assert_eq!(data.x.to_string(), id);
        }
    }

    #[test]
    fn multi_threaded_write_with_single_file() {
        let dir = TempDir::new("test").expect("Could not create temporary directory");
        let file = dir.path().join("db.json");
        let mut cfg = Config::default();
        cfg.single = true;
        let store = Store::new_with_cfg(file, cfg).unwrap();
        multi_threaded_write(store);
    }

    #[test]
    fn multi_threaded_write_with_dir() {
        #[derive(Serialize, Deserialize)]
        struct Data {
            x: i32,
        }
        let dir = TempDir::new("test").expect("Could not create temporary directory");
        let mut cfg = Config::default();
        cfg.single = false;
        let store = Store::new_with_cfg(dir.path(), cfg).unwrap();
        multi_threaded_write(store);
    }

    #[test]
    fn multi_threaded_write_in_memory() {
        #[derive(Serialize, Deserialize)]
        struct Data {
            x: i32,
        }
        let store = Store::new(IN_MEMORY).unwrap();
        multi_threaded_write(store);
    }
}
