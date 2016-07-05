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

## License

This project is licensed under the MIT License.
