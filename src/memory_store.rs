use std::{
    collections::{BTreeMap, HashMap},
    io::{Error, ErrorKind, Result},
    sync::Arc,
};

use uuid::Uuid;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::json_store::JsonStore;

#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    mem: Arc<RwLock<HashMap<String, Mutex<String>>>>,
}

impl JsonStore for MemoryStore {
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
        let json = serde_json::to_string(&obj).map_err(|err| Error::new(ErrorKind::Other, err))?;
        let map = self.mem.read();
        if let Some(val) = map.get(id) {
            let mut value_guard = val.lock();
            *value_guard = json;
            return Ok(id.to_owned());
        }
        drop(map);
        let mut map = self.mem.write();
        map.insert(id.to_owned(), Mutex::new(json));
        Ok(id.to_owned())
    }

    fn get<T>(&self, id: &str) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let map = self.mem.read();
        let value = map
            .get(id)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "no such object"))?;
        let value_guard = value.lock();
        serde_json::from_str(&value_guard).map_err(|err| Error::new(ErrorKind::Other, err))
    }

    fn all<T>(&self) -> Result<BTreeMap<String, T>>
    where
        for<'de> T: Deserialize<'de>,
    {
        let mut result = BTreeMap::new();
        let map = self.mem.read();
        for x in map.iter() {
            let (k, v) = x;
            let value_guard = v.lock();
            if let Ok(r) = serde_json::from_str(&value_guard) {
                result.insert(k.clone(), r);
            }
        }
        Ok(result)
    }

    fn delete(&self, id: &str) -> Result<()> {
        let mut map = self.mem.write();
        if map.contains_key(id) {
            map.remove(id);
        } else {
            return Err(Error::new(ErrorKind::NotFound, "no such object"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_derive::{Deserialize, Serialize};
    use std::thread;

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

    #[test]
    fn save() {
        let db = MemoryStore::default();
        let data = X { x: 56 };
        let id = db.save(&data).unwrap();
        assert_eq!(db.mem.read().len(), 1);
        let json = db.mem.read().get(&id).unwrap().lock().clone();
        assert_eq!(json, "{\"x\":56}");
    }

    #[test]
    fn update() {
        let db = MemoryStore::default();
        let mut data = X { x: 56 };
        let id = db.save(&data).unwrap();
        let json = db.mem.read().get(&id).unwrap().lock().clone();
        assert_eq!(json, "{\"x\":56}");
        data.x += 1;
        db.save_with_id(&data, &id).unwrap();
        let json = db.mem.read().get(&id).unwrap().lock().clone();
        assert_eq!(json, "{\"x\":57}");
    }

    #[test]
    fn save_and_read_multi_threaded() {
        let db = MemoryStore::default();
        let mut threads: Vec<thread::JoinHandle<()>> = vec![];
        let x = X { x: 56 };
        db.save_with_id(&x, "bla").unwrap();
        for i in 0..20 {
            let x = X { x: i };
            let db_clone = db.clone();
            threads.push(thread::spawn(move || {
                db_clone.save_with_id(&x, "bla").unwrap();
            }));
        }
        for _ in 0..20 {
            let db_clone = db.clone();
            threads.push(thread::spawn(move || {
                db_clone.get::<X>("bla").unwrap();
            }));
        }
        for c in threads {
            c.join().unwrap();
        }
    }

    #[test]
    fn save_empty_obj() {
        let db = MemoryStore::default();
        let id = db.save(&Empty {}).unwrap();
        let json = db.mem.read().get(&id).unwrap().lock().clone();
        assert_eq!(json, "{}");
    }

    #[test]
    fn save_with_id() {
        let db = MemoryStore::default();
        let data = Y { y: -7 };
        db.save_with_id(&data, "foo").unwrap();
        let json = db.mem.read().get("foo").unwrap().lock().clone();
        assert_eq!(json, "{\"y\":-7}");
    }

    #[test]
    fn get() {
        let db = MemoryStore::default();
        db.mem
            .write()
            .insert("foo".to_string(), Mutex::new("{\"z\":9.9}".to_string()));
        let obj: Z = db.get("foo").unwrap();
        assert_eq!(obj.z, 9.9);
    }

    #[test]
    fn get_non_existent() {
        let db = MemoryStore::default();
        let res = db.get::<X>("foobarobject");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn all() {
        let db = MemoryStore::default();

        #[derive(Deserialize, Serialize)]
        struct X {
            x: u32,
            y: u32,
        }
        db.mem.write().insert(
            "foo".to_string(),
            Mutex::new("{\"x\":1,\"y\":0}".to_string()),
        );
        db.mem
            .write()
            .insert("bar".to_string(), Mutex::new("{\"y\":2}".to_string()));

        let all_x: BTreeMap<String, X> = db.all().unwrap();
        let all_y: BTreeMap<String, Y> = db.all().unwrap();
        assert_eq!(all_x.get("foo").unwrap().x, 1);
        assert!(all_x.get("bar").is_none());
        assert_eq!(all_y.get("bar").unwrap().y, 2);
    }

    #[test]
    fn delete() {
        let db = MemoryStore::default();
        let data = Y { y: 88 };
        let id = db.save(&data).unwrap();
        db.get::<Y>(&id).unwrap();
        assert_eq!(db.mem.read().len(), 1);
        db.delete(&id).unwrap();
        assert_eq!(db.mem.read().len(), 0);
        assert!(db.get::<Y>(&id).is_err());
        assert!(db.delete(&id).is_err());
    }

    #[test]
    fn delete_non_existent() {
        let db = MemoryStore::default();
        let res = db.delete("blabla");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().kind(), ErrorKind::NotFound);
    }
}
