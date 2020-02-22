## v0.7.0 (unreleased)

- Change License to MIT/Apache 2.0
- use uuid v0.8.x

## v0.6.2 (2019-09-01)

- Use RwLock to allow multithreaded access

## v0.6.1 (2019-08-30)

- add in-memory support

## v0.6.0 (2019-08-06)

- create temporary files within the same directory like the target file
- switched to Rust edition 2018
- use uuid v0.7.x

## v0.5.0 (2018-04-27)

- use uuid v0.6.x
- add method `path()` to access storage location
- generify constructors path argument

## v0.4.0 (2017-05-31)

- use serde v1.x
- use uuid v0.5.x

## v0.3.0 (2017-02-04)

- BREAKING changes:
  - renamed `get_all()` to `all()`
  - dropped `rustc_serialize` support
- use serde v0.9.x

## v0.2.2 (2016-09-24)

- derive `Clone` for `Store`

## v0.2.1 (2016-07-19)

- new: support serde v0.7.x

## v0.2.0 (2016-07-12)

- new: lock files during write
- fix: don't return an error if a directory already exists

## v0.1.1 (2016-07-12)

- make configuration fields (`pretty`, `indent`, `single`) public

## v0.1.0 (2016-07-03)

- Initial release
