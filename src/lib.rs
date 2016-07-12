// Copyright (c) 2016 Markus Kohlhase <mail@markus-kohlhase.de>

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
extern crate rustc_serialize;

use std::io::prelude::*;
use std::io::{Error, ErrorKind, Result};
use uuid::Uuid;
use rustc_serialize::{Decodable, Encodable};
use rustc_serialize::json::{self, Json};
use std::path::PathBuf;
use std::fs::{read_dir, rename, create_dir_all, remove_file, File, metadata};
use std::collections::BTreeMap;

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

    fn save_object_to_file<T: Encodable>(&self, obj: &T, file_name: &PathBuf) -> Result<()> {
        let json_string = if self.cfg.pretty {
            json::as_pretty_json(&obj).indent(self.cfg.indent).to_string()
        } else {
            try!(json::encode(&obj).map_err(|err| Error::new(ErrorKind::InvalidData, err)))
        };

        let tmp_filename = file_name.with_extension("tmp");

        let mut file = try!(File::create(&tmp_filename));

        match Write::write_all(&mut file, json_string.as_bytes()) {
            Err(err) => Err(err),
            Ok(_) => {
                try!(rename(tmp_filename, file_name));
                Ok(())
            }
        }
    }

    fn get_json_from_file(file_name: &PathBuf) -> Result<Json> {
        let mut f = try!(File::open(file_name));
        let mut buffer = String::new();
        try!(f.read_to_string(&mut buffer));
        json::Json::from_str(&buffer).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    fn get_object_from_json(json: &Json) -> Result<&json::Object> {
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
            let o = json::Object::new();
            try!(s.save_object_to_file(&o, &s.path));
        } else {
            try!(create_dir_all(&s.path));
        }
        Ok(s)
    }

    pub fn save<T: Encodable>(&self, obj: &T) -> Result<String> {
        self.save_with_id(obj, &Uuid::new_v4().to_string())
    }

    pub fn save_with_id<T: Encodable>(&self, obj: &T, id: &str) -> Result<String> {
        if self.cfg.single {

            let json = try!(Store::get_json_from_file(&self.path));
            let o = try!(Store::get_object_from_json(&json));
            let mut x = o.clone();

            // start dirty
            let s = try!(json::encode(&obj).map_err(|err| Error::new(ErrorKind::InvalidData, err)));
            let j = try!(Json::from_str(&s).map_err(|err| Error::new(ErrorKind::InvalidData, err)));
            // end dirty

            x.insert(id.to_string(), j);
            try!(self.save_object_to_file(&x, &self.path));

        } else {
            try!(self.save_object_to_file(obj, &self.id_to_path(id)));
        }
        Ok(id.to_owned())
    }

    pub fn get<T: Decodable>(&self, id: &str) -> Result<T> {
        let json = try!(Store::get_json_from_file(&self.id_to_path(id)));
        let o = if self.cfg.single {
            let x = try!(json.find(id).ok_or(Error::new(ErrorKind::NotFound, "no such object")));
            x.clone()
        } else {
            json
        };

        T::decode(&mut json::Decoder::new(o)).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    pub fn get_all<T: Decodable>(&self) -> Result<BTreeMap<String, T>> {
        if self.cfg.single {
            let json = try!(Store::get_json_from_file(&self.id_to_path("")));
            let o = try!(Store::get_object_from_json(&json));
            let mut result = BTreeMap::new();
            for x in o.iter() {
                let (k, v) = x;
                if let Ok(r) = T::decode(&mut json::Decoder::new(v.clone())) {
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

#[cfg(test)]
mod tests {
    use std::io::prelude::*;
    use std::fs::{remove_dir_all, File, remove_file};
    use Store;
    use Config;
    use std::collections::BTreeMap;
    use std::io::{Result, ErrorKind};
    use std::path::Path;
    use uuid::Uuid;

    #[derive(RustcEncodable,RustcDecodable)]
    struct X {
        x: u32,
    }

    #[derive(RustcEncodable,RustcDecodable)]
    struct Y {
        y: u32,
    }

    fn write_to_test_file(name: &str, content: &str) {
        let mut file = File::create(&name).unwrap();
        Write::write_all(&mut file, content.as_bytes()).unwrap();
    }

    fn read_from_test_file(name: &str) -> String {
        let mut f = File::open(name).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        buffer
    }

    fn teardown(dir: &str) -> Result<()> {
        let p = Path::new(dir);
        if p.is_file() {
            match remove_file(p) {
                Err(err) => {
                    match err.kind() {
                        ErrorKind::NotFound => Ok(()),
                        _ => Err(err),
                    }
                },
                Ok(_) => Ok(())
            }
        } else {
            match remove_dir_all(dir) {
                Err(err) => {
                    match err.kind() {
                        ErrorKind::NotFound => Ok(()),
                        _ => Err(err),
                    }
                },
                Ok(_) => Ok(())
            }
        }
    }

    #[test]
    fn save() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        #[derive(RustcEncodable)]
        struct MyData {
            x: u32,
        };
        let data = MyData { x: 56 };
        let id = db.save(&data).unwrap();
        let mut f = File::open(format!("{}/{}.json", dir, id)).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "{\"x\":56}");
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn save_empty_obj() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        #[derive(RustcEncodable)]
        struct Empty {};
        let id = db.save(&Empty {}).unwrap();
        let mut f = File::open(format!("{}/{}.json", dir, id)).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "{}");
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn save_with_id() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        #[derive(RustcEncodable)]
        struct MyData {
            y: i32,
        };
        let data = MyData { y: -7 };
        db.save_with_id(&data, "foo").unwrap();
        let mut f = File::open(format!("{}/foo.json", dir)).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "{\"y\":-7}");
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn pretty_print_file_content() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.pretty = true;
        let db = Store::new_with_cfg(&dir, cfg).unwrap();

        #[derive(RustcEncodable)]
        struct SubStruct {
            c: u32,
        };

        #[derive(RustcEncodable)]
        struct MyData {
            a: String,
            b: SubStruct,
        };

        let data = MyData {
            a: "foo".to_string(),
            b: SubStruct { c: 33 },
        };

        let id = db.save(&data).unwrap();
        let mut f = File::open(format!("{}/{}.json", dir, id)).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        let expected = "{\n  \"a\": \"foo\",\n  \"b\": {\n    \"c\": 33\n  }\n}";
        assert_eq!(buffer, expected);
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn get() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        #[derive(RustcDecodable)]
        struct MyData {
            z: f32,
        };
        let mut file = File::create(format!("{}/foo.json", dir)).unwrap();
        Write::write_all(&mut file, "{\"z\":9.9}".as_bytes()).unwrap();
        let obj: MyData = db.get("foo").unwrap();
        assert_eq!(obj.z, 9.9);
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn get_non_existent() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        let res = db.get::<X>("foobarobject");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn get_all() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        #[derive(RustcEncodable,RustcDecodable)]
        struct X {
            x: u32,
            y: u32,
        };

        let mut file = File::create(format!("{}/foo.json", dir)).unwrap();
        Write::write_all(&mut file, "{\"x\":1, \"y\":0}".as_bytes()).unwrap();

        let mut file = File::create(format!("{}/bar.json", dir)).unwrap();
        Write::write_all(&mut file, "{\"y\":2}".as_bytes()).unwrap();

        let all_x: BTreeMap<String, X> = db.get_all().unwrap();
        let all_y: BTreeMap<String, Y> = db.get_all().unwrap();
        assert_eq!(all_x.get("foo").unwrap().x, 1);
        assert!(all_x.get("bar").is_none());
        assert_eq!(all_y.get("bar").unwrap().y, 2);
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn delete() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        let data = Y { y: 88 };
        let id = db.save(&data).unwrap();
        let f_name = format!("{}/{}.json", dir, id);
        db.get::<Y>(&id).unwrap();
        assert_eq!(Path::new(&f_name).exists(), true);
        db.delete(&id).unwrap();
        assert_eq!(Path::new(&f_name).exists(), false);
        assert!(db.get::<Y>(&id).is_err());
        assert!(db.delete(&id).is_err());
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn delete_non_existent() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let db = Store::new(&dir).unwrap();
        let res = db.delete("blabla");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn single_save() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        assert_eq!(read_from_test_file(&file_name), "{}");
        let x = X { x: 3 };
        let y = Y { y: 4 };
        db.save_with_id(&x, "x").unwrap();
        db.save_with_id(&y, "y").unwrap();
        assert_eq!(read_from_test_file(&file_name),
                   "{\"x\":{\"x\":3},\"y\":{\"y\":4}}");
        assert!(teardown(&file_name).is_ok());
    }

    #[test]
    fn single_save_without_file_name_ext() {
        let dir = format!(".specTests/{}",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        Store::new_with_cfg(&dir, cfg).unwrap();
        assert!(Path::new(&format!("{}.json", dir)).exists());
        assert!(teardown(&dir).is_ok());
    }

    #[test]
    fn single_get() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        write_to_test_file(&file_name, "{\"x\":{\"x\":8},\"y\":{\"y\":9}}");
        let y = db.get::<Y>("y").unwrap();
        assert_eq!(y.y, 9);
        assert!(teardown(&file_name).is_ok());
    }

    #[test]
    fn single_get_non_existent() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        let res = db.get::<X>("foobarobject");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn single_get_all() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        write_to_test_file(&file_name, "{\"foo\":{\"x\":8},\"bar\":{\"x\":9}}");
        let all: BTreeMap<String, X> = db.get_all().unwrap();
        assert_eq!(all.get("foo").unwrap().x, 8);
        assert_eq!(all.get("bar").unwrap().x, 9);
        assert!(teardown(&file_name).is_ok());
    }


    #[test]
    fn single_delete() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        write_to_test_file(&file_name, "{\"foo\":{\"x\":8},\"bar\":{\"x\":9}}");
        db.delete("bar").unwrap();
        assert_eq!(read_from_test_file(&file_name), "{\"foo\":{\"x\":8}}");
        db.delete("foo").unwrap();
        assert_eq!(read_from_test_file(&file_name), "{}");
        assert!(teardown(&file_name).is_ok());
    }

    #[test]
    fn single_delete_non_existent() {
        let file_name = format!(".specTests/{}.json",Uuid::new_v4());
        let mut cfg = Config::default();
        cfg.single = true;
        let db = Store::new_with_cfg(&file_name, cfg).unwrap();
        let res = db.delete("blabla");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    }

}
