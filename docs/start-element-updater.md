# Start-element updater

`Serializer::set_start_element_updater` installs an optional per-serializer
`FnMut(&mut XmlStartElement) + Send + Sync + 'static` callback. It runs once for each start element,
including a public raw `Serializer::write(XmlEvent::StartElement { .. })` call.
No callback keeps the existing direct writer path.

```rust
use yaserde::ser::Serializer;

let version_uri = String::from("urn:example:v2");
let mut serializer = Serializer::new_for_inner(Vec::new());
serializer.set_start_element_updater(move |start| {
  start.update_namespace("soap", version_uri.as_str());
  start.set_attribute("version", "2");
});
```

The callback must be `Send + Sync + 'static`, so capture thread-safe owned
settings with `move`; this preserves `Serializer`'s existing `Send + Sync`
auto traits, keeps serializer instances independent, and avoids tying a
serializer to a borrowed configuration lifetime. `update_namespace` changes only an existing declaration,
while `set_namespace` inserts or replaces one. Both keep expanded element and
prefixed attribute names in sync; an unprefixed attribute is never placed in the
default namespace. `set_attribute` replaces or inserts an ordinary unprefixed
attribute by local name.

## Performance smoke benchmark

Run the dependency-light, release-mode benchmark with:

```sh
cargo run -p yaserde --example start_element_updater_bench --release \
  --no-default-features --features xml-rs-backend
```

It serializes a derived SOAP/CWMP `Envelope` shaped like Codex's
`GetParameterValuesResponse`: root SOAP/CWMP/SOAP encoding/XMLSchema bindings,
an unprefixed `ParameterList` and `ParameterValueStruct`, and `Value` elements
with optional `xsi:type` attributes and text. The benchmark verifies byte-identical
no-op output, CWMP 1-0 to 1-3 namespace mapping, root bindings, and unchanged
values before running three warmup rounds and seven rotating-order samples. It reports median
nanoseconds per operation and each case's ratio to the no-hook baseline.

Optional positional arguments set parameter-list size and iterations per sample:

```sh
cargo run -p yaserde --example start_element_updater_bench --release \
  --no-default-features --features xml-rs-backend -- 2000 50
```
