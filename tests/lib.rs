extern crate jfs;
extern crate uuid;
#[macro_use]
extern crate serde_derive;

use std::io::prelude::*;
use std::fs::{remove_dir_all, File, remove_file};
use jfs::{Config, Store};
use std::collections::BTreeMap;
use std::io::{Result, ErrorKind};
use std::path::Path;
use uuid::Uuid;
use std::thread;

#[derive(Serialize,Deserialize)]
struct X {
    x: u32,
}

#[derive(Serialize,Deserialize)]
struct Y {
    y: i32,
}

#[derive(Serialize,Deserialize)]
struct Empty {}

#[derive(Serialize,Deserialize)]
struct Z {
    z: f32,
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
            }
            Ok(_) => Ok(()),
        }
    } else {
        match remove_dir_all(dir) {
            Err(err) => {
                match err.kind() {
                    ErrorKind::NotFound => Ok(()),
                    _ => Err(err),
                }
            }
            Ok(_) => Ok(()),
        }
    }
}

#[test]
fn new_multi_threaded() {
    let mut threads: Vec<thread::JoinHandle<()>> = vec![];
    let dir = format!(".specTests-{}", Uuid::new_v4());
    for _ in 0..20 {
        let d = dir.clone();
        threads.push(thread::spawn(move || {
            assert!(Store::new(&d).is_ok());
        }));
    }
    for c in threads {
        c.join().unwrap();
    }
    assert!(teardown(&dir).is_ok());
}

#[test]
fn save() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let data = X { x: 56 };
    let id = db.save(&data).unwrap();
    let mut f = File::open(format!("{}/{}.json", dir, id)).unwrap();
    let mut buffer = String::new();
    f.read_to_string(&mut buffer).unwrap();
    assert_eq!(buffer, "{\"x\":56}");
    assert!(teardown(&dir).is_ok());
}

#[test]
fn save_and_read_multi_threaded() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let mut threads: Vec<thread::JoinHandle<()>> = vec![];
    let x = X { x: 56 };
    db.save_with_id(&x, "bla").unwrap();
    for i in 0..20 {
        let d = dir.clone();
        let x = X { x: i };
        threads.push(thread::spawn(move || {
            let db = Store::new(&d).unwrap();
            db.save_with_id(&x, "bla").unwrap();
        }));
    }
    for _ in 0..20 {
        let d = dir.clone();
        threads.push(thread::spawn(move || {
            let db = Store::new(&d).unwrap();
            db.get::<X>("bla").unwrap();
        }));
    }
    for c in threads {
        c.join().unwrap();
    }
    assert!(teardown(&dir).is_ok());
}

#[test]
fn save_empty_obj() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let id = db.save(&Empty {}).unwrap();
    let mut f = File::open(format!("{}/{}.json", dir, id)).unwrap();
    let mut buffer = String::new();
    f.read_to_string(&mut buffer).unwrap();
    assert_eq!(buffer, "{}");
    assert!(teardown(&dir).is_ok());
}

#[test]
fn save_with_id() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let data = Y { y: -7 };
    db.save_with_id(&data, "foo").unwrap();
    let mut f = File::open(format!("{}/foo.json", dir)).unwrap();
    let mut buffer = String::new();
    f.read_to_string(&mut buffer).unwrap();
    assert_eq!(buffer, "{\"y\":-7}");
    assert!(teardown(&dir).is_ok());
}

#[test]
fn pretty_print_file_content() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.pretty = true;
    let db = Store::new_with_cfg(&dir, cfg).unwrap();

    #[derive(Deserialize,Serialize)]
    struct SubStruct {
        c: u32,
    };

    #[derive(Deserialize,Serialize)]
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
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let mut file = File::create(format!("{}/foo.json", dir)).unwrap();
    Write::write_all(&mut file, "{\"z\":9.9}".as_bytes()).unwrap();
    let obj: Z = db.get("foo").unwrap();
    assert_eq!(obj.z, 9.9);
    assert!(teardown(&dir).is_ok());
}

#[test]
fn get_non_existent() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let res = db.get::<X>("foobarobject");
    assert!(res.is_err());
    assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    assert!(teardown(&dir).is_ok());
}

#[test]
fn all() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();

    #[cfg(feature = "serde_json")]
    #[derive(Deserialize,Serialize)]
    struct X {
        x: u32,
        y: u32,
    };
    #[cfg(feature = "rustc-serialize")]
    #[derive(RustcEncodable,RustcDecodable)]
    struct X {
        x: u32,
        y: u32,
    };

    let mut file = File::create(format!("{}/foo.json", dir)).unwrap();
    Write::write_all(&mut file, "{\"x\":1, \"y\":0}".as_bytes()).unwrap();

    let mut file = File::create(format!("{}/bar.json", dir)).unwrap();
    Write::write_all(&mut file, "{\"y\":2}".as_bytes()).unwrap();

    let all_x: BTreeMap<String, X> = db.all().unwrap();
    let all_y: BTreeMap<String, Y> = db.all().unwrap();
    assert_eq!(all_x.get("foo").unwrap().x, 1);
    assert!(all_x.get("bar").is_none());
    assert_eq!(all_y.get("bar").unwrap().y, 2);
    assert!(teardown(&dir).is_ok());
}

#[test]
fn delete() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
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
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let db = Store::new(&dir).unwrap();
    let res = db.delete("blabla");
    assert!(res.is_err());
    assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    assert!(teardown(&dir).is_ok());
}

#[test]
fn single_new_multi_threaded() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    let mut threads: Vec<thread::JoinHandle<()>> = vec![];
    for _ in 0..20 {
        let n = file_name.clone();
        let c = thread::spawn(move || {
            assert!(Store::new_with_cfg(&n, cfg).is_ok());
        });
        threads.push(c);
    }
    for c in threads {
        c.join().unwrap();
    }
    assert!(teardown(&file_name).is_ok());
}

#[test]
fn single_save() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
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
fn single_save_and_read_multi_threaded() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    let db = Store::new_with_cfg(&file_name, cfg).unwrap();
    let x = X { x: 0 };
    db.save_with_id(&x, "foo").unwrap();
    let mut threads: Vec<thread::JoinHandle<()>> = vec![];
    for i in 1..20 {
        let n = file_name.clone();
        let c = thread::spawn(move || {
            let x = X { x: i };
            let db = Store::new_with_cfg(&n, cfg).unwrap();
            db.save_with_id(&x, "foo").unwrap();
        });
        threads.push(c);
    }
    for _ in 1..20 {
        let n = file_name.clone();
        let c = thread::spawn(move || {
            let db = Store::new_with_cfg(&n, cfg).unwrap();
            db.get::<X>("foo").unwrap();
        });
        threads.push(c);

    }
    for c in threads {
        c.join().unwrap();
    }
    assert!(teardown(&file_name).is_ok());
}

#[test]
fn single_save_without_file_name_ext() {
    let dir = format!(".specTests-{}", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    Store::new_with_cfg(&dir, cfg).unwrap();
    assert!(Path::new(&format!("{}.json", dir)).exists());
    assert!(teardown(&format!("{}.json",dir)).is_ok());
}

#[test]
fn single_get() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
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
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    let db = Store::new_with_cfg(&file_name, cfg).unwrap();
    let res = db.get::<X>("foobarobject");
    assert!(res.is_err());
    assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    assert!(teardown(&file_name).is_ok());
}

#[test]
fn single_all() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    let db = Store::new_with_cfg(&file_name, cfg).unwrap();
    write_to_test_file(&file_name, "{\"foo\":{\"x\":8},\"bar\":{\"x\":9}}");
    let all: BTreeMap<String, X> = db.all().unwrap();
    assert_eq!(all.get("foo").unwrap().x, 8);
    assert_eq!(all.get("bar").unwrap().x, 9);
    assert!(teardown(&file_name).is_ok());
}


#[test]
fn single_delete() {
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
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
    let file_name = format!(".specTests-{}.json", Uuid::new_v4());
    let mut cfg = Config::default();
    cfg.single = true;
    let db = Store::new_with_cfg(&file_name, cfg).unwrap();
    let res = db.delete("blabla");
    assert!(res.is_err());
    assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    assert!(teardown(&file_name).is_ok());
}
