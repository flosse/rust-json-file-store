[![Crates.io](https://img.shields.io/crates/v/jfs.svg)](https://crates.io/crates/jfs)
[![Docs.rs](https://docs.rs/jfs/badge.svg)](https://docs.rs/jfs/)
[![Build status](https://travis-ci.org/flosse/rust-json-file-store.svg?branch=master)](https://travis-ci.org/flosse/rust-json-file-store)
[![Dependency status](https://deps.rs/repo/github/flosse/rust-json-file-store/status.svg)](https://deps.rs/repo/github/flosse/rust-json-file-store)
![License](https://img.shields.io/crates/l/jfs.svg)

# jfs

A simple JSON file store written in Rust.
This is a port of the Node.js library
[json-file-store](https://github.com/flosse/json-file-store/).

**WARNING**:
Don't use it if you want to persist a large amount of objects.
Use a real DB instead.

## Example

```rust
extern crate jfs;
#[macro_use]
extern crate serde_derive;
use jfs::Store;

#[derive(Serialize,Deserialize)]
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

Creating a store instance that is living in the memory can be done like this:

```rust
let db = jfs::Store::new(jfs::IN_MEMORY).unwrap();
```

## License

Copyright (c) 2016 - 2020 Markus Kohlhase

This library is licensed under either of

- Apache License, Version 2.0
	([LICENSE-APACHE](https://github.com/flosse/rust-json-file-store/blob/master/LICENSE-APACHE)
	or
	[apache.org/licenses/LICENSE-2.0](https://apache.org/licenses/LICENSE-2.0))
- MIT license
	([LICENSE-MIT](https://github.com/flosse/rust-json-file-store/blob/master/LICENSE-MIT)
	or
	[opensource.org/licenses/MIT](https://opensource.org/licenses/MIT))

at your option.
