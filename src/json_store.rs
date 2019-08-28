use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Result};

pub(crate) trait JsonStore: Send + Sync {
    fn save<T>(&self, obj: &T) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>;
    fn save_with_id<T>(&self, obj: &T, id: &str) -> Result<String>
    where
        for<'de> T: Serialize + Deserialize<'de>;
    fn get<T>(&self, id: &str) -> Result<T>
    where
        for<'de> T: Deserialize<'de>;
    fn all<T>(&self) -> Result<BTreeMap<String, T>>
    where
        for<'de> T: Deserialize<'de>;
    fn delete(&self, id: &str) -> Result<()>;
}
