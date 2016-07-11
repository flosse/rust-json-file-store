[![](http://meritbadge.herokuapp.com/sun)](https://crates.io/crates/jfs)

# jfs

A simple JSON file store written in Rust.
This is a port and drop-in replacement of the Node.js library
[json-file-store](https://github.com/flosse/json-file-store/).

**WARNING**:
Don't use it if you want to persist a large amount of objects.
Use a real DB instead.

## Example

```rust
extern crate jfs;
extern crate rustc_serialize;
use jfs::Store;

#[derive(RustcEncodable,RustcDecodable)]
struct Foo {
  foo: String
}

pub fn main() {
   let db = Store::new("data").unwrap();
   let f = Foo { foo: "bar".to_owned() };
   let id = db.save(&f).unwrap();
   let obj = db.get::<Foo>(&id).unwrap();
   db.delete(&id).unwrap();
}
```

You can also store all data in one single JSON-File:

```rust
let mut cfg = jfs::Config::default();
cfg.single = true; // false is default
let db = jfs::Store::new_with_cfg("data",cfg);
```

If you like to pretty print the file content, set `pretty` to `true`
and choose a number of whitespaces for the indention:

```rust
let mut cfg = jfs::Config::default();
cfg.pretty = true;  // false is default
cfg.indent = 4;     // 2 is default
```

## License

This project is licensed under the MIT License.
