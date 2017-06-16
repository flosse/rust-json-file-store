// Copyright (c) 2016 - 2017 Markus Kohlhase <mail@markus-kohlhase.de>

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
//!   foo: String
//! }
//!
//! pub fn main() {
//!    let db = Store::new("data").unwrap();
//!    let f = Foo { foo: "bar".to_owned() };
//!    let id = db.save(&f).unwrap();
//!    let obj = db.get::<Foo>(&id).unwrap();
//!    db.delete(&id).unwrap();
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

extern crate uuid;
extern crate fs2;
extern crate serde;
extern crate serde_json;

use std::io::prelude::*;
use std::io::{Error, ErrorKind, Result};
use uuid::Uuid;

use serde::{Serialize, Deserialize};
use serde_json::Value;
use serde_json::value::Map;
use serde_json::ser::{Serializer, PrettyFormatter};

use std::path::{Path, PathBuf};
use std::fs::{read_dir, rename, create_dir_all, remove_file, metadata, OpenOptions};
use std::collections::BTreeMap;
use fs2::FileExt;

type Object = Map<String, Value>;

#[derive(Clone,Copy)]
pub struct Config {
    pub pretty: bool,
    pub indent: u32,
    pub single: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            indent: 2,
            pretty: false,
            single: false,
        }
    }
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    cfg: Config,
}

impl Store {
    fn id_to_path(&self, id: &str) -> PathBuf {
        if self.cfg.single {
            self.path.clone()
        } else {
            self.path.join(id).with_extension("json")
        }
    }

    fn path_buf_to_id(&self, p: PathBuf) -> Result<String> {
        p.file_stem()
            .and_then(|n| n.to_os_string().into_string().ok())
            .ok_or_else(|| Error::new(ErrorKind::Other, "invalid id"))
    }

    fn to_writer_pretty<W: Write, T: Serialize>(&self, writer: &mut W, value: &T) -> Result<()> {
        let mut indent: Vec<char> = vec![];
        for _ in 0..self.cfg.indent {
            indent.push(' ');
        }
        let b = indent.into_iter().collect::<String>().into_bytes();
        let mut s = Serializer::with_formatter(writer, PrettyFormatter::with_indent(&b));
        value
            .serialize(&mut s)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(())
    }

    fn to_vec_pretty<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        let mut writer: Vec<u8> = vec![];
        self.to_writer_pretty(&mut writer, value)?;
        Ok(writer)
    }

    fn object_to_string<T: Serialize>(&self, obj: &T) -> Result<String> {
        if self.cfg.pretty {
            let vec = self.to_vec_pretty(obj)?;
            String::from_utf8(vec).map_err(|err| Error::new(ErrorKind::Other, err))
        } else {
            serde_json::to_string(obj).map_err(|err| Error::new(ErrorKind::Other, err))
        }
    }

    fn save_object_to_file<T: Serialize>(&self, obj: &T, file_name: &PathBuf) -> Result<()> {
        let json_string = self.object_to_string(obj)?;
        let tmp_filename = Path::new(&Uuid::new_v4().to_string()).with_extension("tmp");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&file_name)?;
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_filename)?;
        file.lock_exclusive()?;
        tmp_file.lock_exclusive()?;

        match Write::write_all(&mut tmp_file, json_string.as_bytes()) {
            Err(err) => Err(err),
            Ok(_) => {
                rename(tmp_filename, file_name)?;
                tmp_file.unlock()?;
                file.unlock()
            }
        }
    }

    fn get_string_from_file(file_name: &PathBuf) -> Result<String> {
        let mut f = OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(&file_name)?;
        let mut buffer = String::new();
        f.lock_shared()?;
        f.read_to_string(&mut buffer)?;
        f.unlock()?;
        Ok(buffer)
    }

    fn get_json_from_file(file_name: &PathBuf) -> Result<Value> {
        let s = Store::get_string_from_file(file_name)?;
        serde_json::from_str(&s).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    fn get_object_from_json(json: &Value) -> Result<&Object> {
        json.as_object()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "invalid file content"))
    }

    /// Opens a `Store` against the specified path.
    ///
    /// See `new_with_cfg(..)` for more details
    ///
    /// # Arguments
    /// 
    /// * `path` - path to the db directory of JSON documents
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Store> {
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
    pub fn new_with_cfg<P: AsRef<Path>>(path: P, cfg: Config) -> Result<Store> {

        let mut s = Store {
            path: path.as_ref().to_path_buf(), // TODO: probably change this to take an owned PathBuf parameter
            cfg: cfg,
        };

        if cfg.single {
            s.path = s.path.with_extension("json");
            if !s.path.exists() {
                let o = Object::new();
                s.save_object_to_file(&o, &s.path)?;
            }
        } else if let Err(err) = create_dir_all(&s.path) {
            if err.kind() != ErrorKind::AlreadyExists {
                return Err(err);
            }
        }
        Ok(s)
    }

    /// Returns the storage path for the backing JSON store.
    ///
    /// In single-file-mode this will be the JSON file location, otherwise it's 
    ///  the directory in which all JSON objects are stored. 
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save<T>(&self, obj: &T) -> Result<String>
        where for<'de> T: Serialize + Deserialize<'de>
    {
        self.save_with_id(obj, &Uuid::new_v4().to_string())
    }

    pub fn save_with_id<T>(&self, obj: &T, id: &str) -> Result<String>
        where for<'de> T: Serialize + Deserialize<'de>
    {
        if self.cfg.single {
            let json = Store::get_json_from_file(&self.path)?;
            let o = Store::get_object_from_json(&json)?;
            let mut x = o.clone();
            let j = serde_json::to_value(&obj)
                .map_err(|err| Error::new(ErrorKind::Other, err))?;
            x.insert(id.to_string(), j);
            self.save_object_to_file(&x, &self.path)?;

        } else {
            self.save_object_to_file(obj, &self.id_to_path(id))?;
        }
        Ok(id.to_owned())
    }

    fn decode<T>(o: Value) -> Result<T>
        where for<'de> T: Deserialize<'de>
    {
        serde_json::from_value(o).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    pub fn get<T>(&self, id: &str) -> Result<T>
        where for<'de> T: Deserialize<'de>
    {
        let json = Store::get_json_from_file(&self.id_to_path(id))?;
        let o = if self.cfg.single {
            let x = json.get(id)
                .ok_or_else(||Error::new(ErrorKind::NotFound, "no such object"))?;
            x.clone()
        } else {
            json
        };
        Self::decode(o)
    }

    pub fn all<T>(&self) -> Result<BTreeMap<String, T>>
        where for<'de> T: Deserialize<'de>
    {
        if self.cfg.single {
            let json = Store::get_json_from_file(&self.id_to_path(""))?;
            let o = Store::get_object_from_json(&json)?;
            let mut result = BTreeMap::new();
            for x in o.iter() {
                let (k, v) = x;
                if let Ok(r) = Self::decode(v.clone()) {
                    result.insert(k.clone(), r);
                }
            }
            return Ok(result);
        }

        if !metadata(&self.path)?.is_dir() {
            return Err(Error::new(ErrorKind::NotFound, "invalid path"));
        }

        let entries = read_dir(&self.path)?
            .filter_map(|e|
                e.and_then(|x|
                    x.metadata().and_then(|m|
                        if m.is_file() {
                            self.path_buf_to_id(x.path())
                        } else {
                            Err(Error::new(ErrorKind::Other, "not a file"))
                        }
                    )
                ).ok()
            )
            .filter_map(|id| match self.get(&id) {
                Ok(x) => Some((id.clone(), x)),
                _ => None,
            })
            .collect::<BTreeMap<String, T>>();

        Ok(entries)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if self.cfg.single {
            let json = Store::get_json_from_file(&self.path)?;
            let o = Store::get_object_from_json(&json)?;
            let mut x = o.clone();
            if x.contains_key(id) {
                x.remove(id);
            } else {
                return Err(Error::new(ErrorKind::NotFound, "no such object"));
            }
            self.save_object_to_file(&x, &self.path)
        } else {
            remove_file(self.id_to_path(id))
        }
    }
}
