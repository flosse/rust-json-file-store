// Copyright (c) 2016 Markus Kohlhase <mail@markus-kohlhase.de>

//! A simple JSON file store written in Rust.
//! This is a port and drop-in replacement of the Node.js library
//! [json-file-store](https://github.com/flosse/json-file-store/).
//!
//! **WARNING**:
//! Don't use it if you want to persist a large amount of objects.
//! Use a real DB instead.
//!
//! # Installation
//!
//! Depending on which serialization framework you like to use,
//! you have to enable it by setting the corresponding features
//! in `Cargo.toml`.
//!
//! By default `rustc-serialize` is used. To enable support for serde
//! add the following to your configuration:
//!
//!
//! ```toml
//! [dependencies.jfs]
//! version = "0.2"
//! features = ["serde", "serde_json"]
//! default-features = false
//! ```
//!
//! # Example
//!
//! ```
//! extern crate jfs;
//! extern crate rustc_serialize;
//! use jfs::Store;
//!
//! #[derive(RustcEncodable,RustcDecodable)]
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
//! ```
//! let mut cfg = jfs::Config::default();
//! cfg.single = true; // false is default
//! let db = jfs::Store::new_with_cfg("data",cfg);
//! ```
//!
//! If you like to pretty print the file content, set `pretty` to `true`
//! and choose a number of whitespaces for the indention:
//!
//! ```
//! let mut cfg = jfs::Config::default();
//! cfg.pretty = true;  // false is default
//! cfg.indent = 4;     // 2 is default
//! ```

extern crate uuid;
#[cfg(feature = "rustc-serialize")]
extern crate rustc_serialize;
extern crate fs2;

use std::io::prelude::*;
use std::io::{Error, ErrorKind, Result};
use uuid::Uuid;

#[cfg(feature = "rustc-serialize")]
use rustc_serialize::{Encodable as Serialize, Decodable as Deserialize};
#[cfg(feature = "rustc-serialize")]
use rustc_serialize::json::{self, Json as Value, Object};

#[cfg(feature = "serde")]
extern crate serde;
#[cfg(feature = "serde_json")]
extern crate serde_json;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "serde_json")]
use serde_json::Value;
#[cfg(feature = "serde_json")]
use serde_json::value::Map;
#[cfg(feature = "serde_json")]
use serde_json::ser::{Serializer, PrettyFormatter};

use std::path::{Path, PathBuf};
use std::fs::{read_dir, rename, create_dir_all, remove_file, metadata, OpenOptions};
use std::collections::BTreeMap;
use fs2::FileExt;

#[cfg(feature = "serde_json")]
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

    #[cfg(feature = "serde_json")]
    fn to_writer_pretty<W: Write, T: Serialize>(&self, writer: &mut W, value: &T) -> Result<()> {
        let mut indent: Vec<char> = vec![];
        for _ in 0..self.cfg.indent {
            indent.push(' ');
        }
        let b = indent.into_iter().collect::<String>().into_bytes();
        let mut s = Serializer::with_formatter(writer, PrettyFormatter::with_indent(&b));
        try!(value.serialize(&mut s).map_err(|err| Error::new(ErrorKind::InvalidData, err)));
        Ok(())
    }

    #[cfg(feature = "serde_json")]
    fn to_vec_pretty<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        let mut writer: Vec<u8> = vec![];
        try!(self.to_writer_pretty(&mut writer, value));
        Ok(writer)
    }

    #[cfg(feature = "rustc-serialize")]
    fn object_to_string<T: Serialize>(&self, obj: &T) -> Result<String> {
        if self.cfg.pretty {
            Ok(json::as_pretty_json(&obj).indent(self.cfg.indent).to_string())
        } else {
            json::encode(&obj).map_err(|err| Error::new(ErrorKind::Other, err))
        }
    }

    #[cfg(feature = "serde_json")]
    fn object_to_string<T: Serialize>(&self, obj: &T) -> Result<String> {
        if self.cfg.pretty {
            let vec = try!(self.to_vec_pretty(obj));
            String::from_utf8(vec).map_err(|err| Error::new(ErrorKind::Other, err))
        } else {
            serde_json::to_string(obj).map_err(|err| Error::new(ErrorKind::Other, err))
        }
    }

    fn save_object_to_file<T: Serialize>(&self, obj: &T, file_name: &PathBuf) -> Result<()> {
        let json_string = try!(self.object_to_string(obj));
        let tmp_filename = Path::new(&Uuid::new_v4().to_string()).with_extension("tmp");
        let file =
            try!(OpenOptions::new().write(true).create(true).truncate(false).open(&file_name));
        let mut tmp_file =
            try!(OpenOptions::new().write(true).create(true).truncate(true).open(&tmp_filename));

        try!(file.lock_exclusive());
        try!(tmp_file.lock_exclusive());

        match Write::write_all(&mut tmp_file, json_string.as_bytes()) {
            Err(err) => Err(err),
            Ok(_) => {
                try!(rename(tmp_filename, file_name));
                try!(tmp_file.unlock());
                file.unlock()
            }
        }
    }

    fn get_string_from_file(file_name: &PathBuf) -> Result<String> {
        let mut f = try!(OpenOptions::new().read(true).write(false).create(false).open(&file_name));
        let mut buffer = String::new();
        try!(f.lock_shared());
        try!(f.read_to_string(&mut buffer));
        try!(f.unlock());
        Ok(buffer)
    }

    #[cfg(feature = "serde_json")]
    fn get_json_from_file(file_name: &PathBuf) -> Result<Value> {
        let s = try!(Store::get_string_from_file(file_name));
        serde_json::from_str(&s).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    #[cfg(feature = "rustc-serialize")]
    fn get_json_from_file(file_name: &PathBuf) -> Result<Value> {
        let s = try!(Store::get_string_from_file(file_name));
        Value::from_str(&s).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    fn get_object_from_json(json: &Value) -> Result<&Object> {
        json.as_object().ok_or_else(|| Error::new(ErrorKind::InvalidData, "invalid file content"))
    }

    pub fn new(name: &str) -> Result<Store> {
        Store::new_with_cfg(name, Config::default())
    }

    pub fn new_with_cfg(name: &str, cfg: Config) -> Result<Store> {

        let mut s = Store {
            path: name.into(),
            cfg: cfg,
        };

        if cfg.single {
            s.path = s.path.with_extension("json");
            if !s.path.exists() {
                let o = Object::new();
                try!(s.save_object_to_file(&o, &s.path));
            }
        } else if let Err(err) = create_dir_all(&s.path) {
            if err.kind() != ErrorKind::AlreadyExists {
                return Err(err);
            }
        }
        Ok(s)
    }

    pub fn save<T: Serialize + Deserialize>(&self, obj: &T) -> Result<String> {
        self.save_with_id(obj, &Uuid::new_v4().to_string())
    }

    #[cfg(feature = "rustc-serialize")]
    fn to_json_value<T: Serialize>(obj: &T) -> Result<Value> {
        // start dirty
        let s = try!(json::encode(&obj).map_err(|err| Error::new(ErrorKind::InvalidData, err)));
        Value::from_str(&s).map_err(|err| Error::new(ErrorKind::InvalidData, err))
        // end dirty
    }

    #[cfg(feature = "serde_json")]
    fn to_json_value<T: Serialize>(obj: &T) -> Result<Value> {
        Ok(serde_json::to_value(&obj))
    }

    pub fn save_with_id<T: Serialize + Deserialize>(&self, obj: &T, id: &str) -> Result<String> {
        if self.cfg.single {
            let json = try!(Store::get_json_from_file(&self.path));
            let o = try!(Store::get_object_from_json(&json));
            let mut x = o.clone();
            let j = try!(Self::to_json_value(obj));
            x.insert(id.to_string(), j);
            try!(self.save_object_to_file(&x, &self.path));

        } else {
            try!(self.save_object_to_file(obj, &self.id_to_path(id)));
        }
        Ok(id.to_owned())
    }

    #[cfg(feature = "rustc-serialize")]
    fn decode<T: Deserialize>(o: Value) -> Result<T> {
        T::decode(&mut json::Decoder::new(o)).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    #[cfg(feature = "serde_json")]
    fn decode<T: Deserialize>(o: Value) -> Result<T> {
        serde_json::from_value(o).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    pub fn get<T: Deserialize>(&self, id: &str) -> Result<T> {
        let json = try!(Store::get_json_from_file(&self.id_to_path(id)));
        let o = if self.cfg.single {
            let x = try!(json.find(id).ok_or(Error::new(ErrorKind::NotFound, "no such object")));
            x.clone()
        } else {
            json
        };
        Self::decode(o)
    }

    pub fn get_all<T: Deserialize>(&self) -> Result<BTreeMap<String, T>> {
        if self.cfg.single {
            let json = try!(Store::get_json_from_file(&self.id_to_path("")));
            let o = try!(Store::get_object_from_json(&json));
            let mut result = BTreeMap::new();
            for x in o.iter() {
                let (k, v) = x;
                if let Ok(r) = Self::decode(v.clone()) {
                    result.insert(k.clone(), r);
                }
            }
            Ok(result)
        } else {
            let meta = try!(metadata(&self.path));
            if !meta.is_dir() {
                Err(Error::new(ErrorKind::NotFound, "invalid path"))
            } else {
                let entries = try!(read_dir(&self.path));
                Ok(entries.filter_map(|e| {
                        e.and_then(|x| {
                                x.metadata().and_then(|m| {
                                    if m.is_file() {
                                        self.path_buf_to_id(x.path())
                                    } else {
                                        Err(Error::new(ErrorKind::Other, "not a file"))
                                    }
                                })
                            })
                            .ok()
                    })
                    .filter_map(|id| match self.get(&id) {
                        Ok(x) => Some((id.clone(), x)),
                        _ => None,
                    })
                    .collect::<BTreeMap<String, T>>())
            }
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if self.cfg.single {
            let json = try!(Store::get_json_from_file(&self.path));
            let o = try!(Store::get_object_from_json(&json));
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
