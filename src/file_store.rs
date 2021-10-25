use crate::json_store::JsonStore;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{
    ser::{PrettyFormatter, Serializer},
    value::Map,
    Value,
};
use std::{
    collections::BTreeMap,
    fs::{create_dir_all, metadata, read_dir, remove_file, rename, OpenOptions},
    io::{
        prelude::*,
        {Error, ErrorKind, Result},
    },
    path::{Path, PathBuf},
};
use uuid::Uuid;

type Object = Map<String, Value>;

#[derive(Clone, Copy)]
pub struct Config {
    pub pretty: bool,
    pub indent: usize,
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
pub struct FileStore {
    path: PathBuf,
    cfg: Config,
}

impl JsonStore for FileStore {
    fn save<T>(&self, obj: &T) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        self.save_with_id(obj, &Uuid::new_v4().to_string())
    }

    fn save_with_id<T>(&self, obj: &T, id: &str) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        if self.cfg.single {
            let json = FileStore::get_json_from_file(&self.path)?;
            let o = FileStore::get_object_from_json(&json)?;
            let mut x = o.clone();
            let j = serde_json::to_value(&obj).map_err(|err| Error::new(ErrorKind::Other, err))?;
            x.insert(id.to_string(), j);
            self.save_object_to_file(&x, &self.path)?;
        } else {
            self.save_object_to_file(obj, &self.id_to_path(id))?;
        }
        Ok(id.to_owned())
    }

    fn get<T>(&self, id: &str) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let json = FileStore::get_json_from_file(&self.id_to_path(id))?;
        let o = if self.cfg.single {
            let x = json
                .get(id)
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such object"))?;
            x.clone()
        } else {
            json
        };
        Self::decode(o)
    }

    fn all<T>(&self) -> Result<BTreeMap<String, T>>
    where
        for<'de> T: Deserialize<'de>,
    {
        if self.cfg.single {
            let json = FileStore::get_json_from_file(&self.id_to_path(""))?;
            let o = FileStore::get_object_from_json(&json)?;
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
            .filter_map(|e| {
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
            .collect::<BTreeMap<String, T>>();

        Ok(entries)
    }

    fn delete(&self, id: &str) -> Result<()> {
        if self.cfg.single {
            let json = FileStore::get_json_from_file(&self.path)?;
            let o = FileStore::get_object_from_json(&json)?;
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

impl FileStore {
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
        let indent = vec![' '; self.cfg.indent];
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

    fn save_object_to_file<T: Serialize>(&self, obj: &T, file_name: &Path) -> Result<()> {
        let json_string = self.object_to_string(obj)?;
        let mut tmp_filename = file_name.to_path_buf();
        tmp_filename.set_file_name(&Uuid::new_v4().to_string());
        tmp_filename.set_extension("tmp");
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

    fn get_string_from_file(file_name: &Path) -> Result<String> {
        let mut f = OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(file_name)?;
        let mut buffer = String::new();
        f.lock_shared()?;
        f.read_to_string(&mut buffer)?;
        f.unlock()?;
        Ok(buffer)
    }

    fn get_json_from_file(file_name: &Path) -> Result<Value> {
        let s = FileStore::get_string_from_file(file_name)?;
        serde_json::from_str(&s).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    fn get_object_from_json(json: &Value) -> Result<&Object> {
        json.as_object()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "invalid file content"))
    }

    #[cfg(test)]
    fn new<P: AsRef<Path>>(path: P) -> Result<FileStore> {
        FileStore::new_with_cfg(path, Config::default())
    }

    pub fn new_with_cfg<P: AsRef<Path>>(path: P, cfg: Config) -> Result<FileStore> {
        let mut s = FileStore {
            path: path.as_ref().to_path_buf(), // TODO: probably change this to take an owned PathBuf parameter
            cfg,
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
    /// the directory in which all JSON objects are stored.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn decode<T>(o: Value) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        serde_json::from_value(o).map_err(|err| Error::new(ErrorKind::Other, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_derive::{Deserialize, Serialize};
    use std::{collections::BTreeMap, fs::File, io::ErrorKind, path::Path, thread};
    use tempdir::TempDir;

    #[derive(Serialize, Deserialize)]
    struct X {
        x: u32,
    }

    #[derive(Serialize, Deserialize)]
    struct Y {
        y: i32,
    }

    #[derive(Serialize, Deserialize)]
    struct Empty {}

    #[derive(Serialize, Deserialize)]
    struct Z {
        z: f32,
    }

    fn write_to_test_file(name: &Path, content: &str) {
        let mut file = File::create(&name).unwrap();
        Write::write_all(&mut file, content.as_bytes()).unwrap();
    }

    fn read_from_test_file(name: &Path) -> String {
        let mut f = File::open(name).unwrap();
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).unwrap();
        buffer
    }

    mod json_store {

        use super::*;

        #[test]
        fn new_multi_threaded() {
            let mut threads: Vec<thread::JoinHandle<()>> = vec![];
            let dir = TempDir::new("tests").unwrap();
            let path = dir.path().to_path_buf();
            for _ in 0..20 {
                let d = path.clone();
                threads.push(thread::spawn(move || {
                    assert!(FileStore::new(&d).is_ok());
                }));
            }
            for c in threads {
                c.join().unwrap();
            }
        }

        #[test]
        fn save() {
            let dir = TempDir::new("tests").unwrap();
            let db = FileStore::new(&dir).unwrap();
            let data = X { x: 56 };
            let id = db.save(&data).unwrap();
            let mut f = File::open(dir.path().join(id).with_extension("json")).unwrap();
            let mut buffer = String::new();
            f.read_to_string(&mut buffer).unwrap();
            assert_eq!(buffer, "{\"x\":56}");
        }

        #[test]
        fn save_and_read_multi_threaded() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let mut threads: Vec<thread::JoinHandle<()>> = vec![];
            let x = X { x: 56 };
            db.save_with_id(&x, "bla").unwrap();
            for i in 0..20 {
                let d = dir.clone();
                let x = X { x: i };
                threads.push(thread::spawn(move || {
                    let db = FileStore::new(&d).unwrap();
                    db.save_with_id(&x, "bla").unwrap();
                }));
            }
            for _ in 0..20 {
                let d = dir.clone();
                threads.push(thread::spawn(move || {
                    let db = FileStore::new(&d).unwrap();
                    db.get::<X>("bla").unwrap();
                }));
            }
            for c in threads {
                c.join().unwrap();
            }
        }

        #[test]
        fn save_empty_obj() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let id = db.save(&Empty {}).unwrap();
            let mut f = File::open(dir.join(id).with_extension("json")).unwrap();
            let mut buffer = String::new();
            f.read_to_string(&mut buffer).unwrap();
            assert_eq!(buffer, "{}");
        }

        #[test]
        fn save_with_id() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let data = Y { y: -7 };
            db.save_with_id(&data, "foo").unwrap();
            let mut f = File::open(dir.join("foo.json")).unwrap();
            let mut buffer = String::new();
            f.read_to_string(&mut buffer).unwrap();
            assert_eq!(buffer, "{\"y\":-7}");
        }

        #[test]
        fn pretty_print_file_content() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let mut cfg = Config::default();
            cfg.pretty = true;
            let db = FileStore::new_with_cfg(&dir, cfg).unwrap();

            #[derive(Deserialize, Serialize)]
            struct SubStruct {
                c: u32,
            }

            #[derive(Deserialize, Serialize)]
            struct MyData {
                a: String,
                b: SubStruct,
            }

            let data = MyData {
                a: "foo".to_string(),
                b: SubStruct { c: 33 },
            };

            let id = db.save(&data).unwrap();
            let mut f = File::open(dir.join(id).with_extension("json")).unwrap();
            let mut buffer = String::new();
            f.read_to_string(&mut buffer).unwrap();
            let expected = "{\n  \"a\": \"foo\",\n  \"b\": {\n    \"c\": 33\n  }\n}";
            assert_eq!(buffer, expected);
        }

        #[test]
        fn get() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let mut file = File::create(dir.join("foo.json")).unwrap();
            Write::write_all(&mut file, b"{\"z\":9.9}").unwrap();
            let obj: Z = db.get("foo").unwrap();
            assert_eq!(obj.z, 9.9);
        }

        #[test]
        fn get_non_existent() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let res = db.get::<X>("foobarobject");
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
        }

        #[test]
        fn all() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();

            #[cfg(feature = "serde_json")]
            #[derive(Deserialize, Serialize)]
            struct X {
                x: u32,
                y: u32,
            }

            let mut file = File::create(dir.join("foo.json")).unwrap();
            Write::write_all(&mut file, b"{\"x\":1, \"y\":0}").unwrap();

            let mut file = File::create(dir.join("bar.json")).unwrap();
            Write::write_all(&mut file, b"{\"y\":2}").unwrap();

            let all_x: BTreeMap<String, X> = db.all().unwrap();
            let all_y: BTreeMap<String, Y> = db.all().unwrap();
            assert_eq!(all_x.get("foo").unwrap().x, 1);
            assert!(all_x.get("bar").is_none());
            assert_eq!(all_y.get("bar").unwrap().y, 2);
        }

        #[test]
        fn delete() {
            let dir = TempDir::new("tests").unwrap();
            let db = FileStore::new(&dir).unwrap();
            let data = Y { y: 88 };
            let id = db.save(&data).unwrap();
            let f_name = dir.path().join(&id).with_extension("json");
            db.get::<Y>(&id).unwrap();
            assert_eq!(Path::new(&f_name).exists(), true);
            db.delete(&id).unwrap();
            assert_eq!(Path::new(&f_name).exists(), false);
            assert!(db.get::<Y>(&id).is_err());
            assert!(db.delete(&id).is_err());
        }

        #[test]
        fn delete_non_existent() {
            let dir = TempDir::new("tests").unwrap().path().to_path_buf();
            let db = FileStore::new(&dir).unwrap();
            let res = db.delete("blabla");
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
        }

        #[test]
        fn single_new_multi_threaded() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let mut threads: Vec<thread::JoinHandle<()>> = vec![];
            for _ in 0..20 {
                let n = file_name.clone();
                let c = thread::spawn(move || {
                    assert!(FileStore::new_with_cfg(&n, cfg).is_ok());
                });
                threads.push(c);
            }
            for c in threads {
                c.join().unwrap();
            }
        }

        #[test]
        fn single_save() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(&file_name, cfg).unwrap();
            assert_eq!(read_from_test_file(&file_name), "{}");
            let x = X { x: 3 };
            let y = Y { y: 4 };
            db.save_with_id(&x, "x").unwrap();
            db.save_with_id(&y, "y").unwrap();
            assert_eq!(
                read_from_test_file(&file_name),
                "{\"x\":{\"x\":3},\"y\":{\"y\":4}}"
            );
        }

        #[test]
        fn single_save_and_read_multi_threaded() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(file_name.clone(), cfg).unwrap();
            let x = X { x: 0 };
            db.save_with_id(&x, "foo").unwrap();
            let mut threads: Vec<thread::JoinHandle<()>> = vec![];
            for i in 1..20 {
                let n = file_name.clone();
                let c = thread::spawn(move || {
                    let x = X { x: i };
                    let db = FileStore::new_with_cfg(&n, cfg).unwrap();
                    db.save_with_id(&x, "foo").unwrap();
                });
                threads.push(c);
            }
            for _ in 1..20 {
                let n = file_name.clone();
                let c = thread::spawn(move || {
                    let db = FileStore::new_with_cfg(&n, cfg).unwrap();
                    db.get::<X>("foo").unwrap();
                });
                threads.push(c);
            }
            for c in threads {
                c.join().unwrap();
            }
        }

        #[test]
        fn single_save_without_file_name_ext() {
            let dir = TempDir::new("tests").unwrap();
            let subdir = dir.path().join("test");
            let mut cfg = Config::default();
            cfg.single = true;
            FileStore::new_with_cfg(&subdir, cfg).unwrap();
            assert!(Path::new(&format!("{}.json", subdir.to_str().unwrap())).exists());
        }

        #[test]
        fn single_get() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(&file_name, cfg).unwrap();
            write_to_test_file(&file_name, "{\"x\":{\"x\":8},\"y\":{\"y\":9}}");
            let y = db.get::<Y>("y").unwrap();
            assert_eq!(y.y, 9);
        }

        #[test]
        fn single_get_non_existent() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(&file_name, cfg).unwrap();
            let res = db.get::<X>("foobarobject");
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
        }

        #[test]
        fn single_all() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(&file_name, cfg).unwrap();
            write_to_test_file(&file_name, "{\"foo\":{\"x\":8},\"bar\":{\"x\":9}}");
            let all: BTreeMap<String, X> = db.all().unwrap();
            assert_eq!(all.get("foo").unwrap().x, 8);
            assert_eq!(all.get("bar").unwrap().x, 9);
        }

        #[test]
        fn single_delete() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(file_name.clone(), cfg).unwrap();
            write_to_test_file(&file_name, "{\"foo\":{\"x\":8},\"bar\":{\"x\":9}}");
            db.delete("bar").unwrap();
            assert_eq!(read_from_test_file(&file_name), "{\"foo\":{\"x\":8}}");
            db.delete("foo").unwrap();
            assert_eq!(read_from_test_file(&file_name), "{}");
        }

        #[test]
        fn single_delete_non_existent() {
            let dir = TempDir::new("tests").unwrap();
            let file_name = dir.path().join("test.json");
            let mut cfg = Config::default();
            cfg.single = true;
            let db = FileStore::new_with_cfg(&file_name, cfg).unwrap();
            let res = db.delete("blabla");
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
        }
    }
}
