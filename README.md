# JSON file store

A simple JSON file store written in Rust.
This is a port and drop-in replacement of the Node.js library
[json-file-store](https://github.com/flosse/json-file-store/).

WARNING:
Don't use it if you want to persist a large amount of objects.
Use a real DB instead.

## Usage

Add the following to your `Cargo.toml`

    [dependencies]
    jfs = "0.1"

## License

This project is licensed under the MIT License.
