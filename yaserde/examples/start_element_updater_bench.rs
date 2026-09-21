use std::env;
use std::hint::black_box;
use std::io::Cursor;
use std::time::Instant;
use yaserde::ser::Serializer;
use yaserde::xml::{XmlEventReader, XmlReadEvent, XmlRsReader};
use yaserde::YaSerialize;
#[cfg(not(feature = "derive"))]
use yaserde_derive::YaSerialize;

const FOO_V1: &str = "urn:example:foo:v1";
const FOO_V2: &str = "urn:example:foo:v2";
const BAR: &str = "urn:example:bar";

#[derive(YaSerialize)]
#[yaserde(prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1", "bar" = "urn:example:bar" })]
struct Foo {
  #[yaserde(rename = "Bar", prefix = "foo")]
  bar: Bar,
}

#[derive(YaSerialize)]
#[yaserde(namespaces = { "foo" = "urn:example:foo:v1", "bar" = "urn:example:bar" })]
struct Bar {
  #[yaserde(attribute = true, prefix = "bar", rename = "version")]
  version: String,
  #[yaserde(rename = "Baz", prefix = "foo")]
  baz: Baz,
}

#[derive(YaSerialize)]
#[yaserde(prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1", "bar" = "urn:example:bar" })]
struct Baz {
  #[yaserde(rename = "Items")]
  items: Items,
}

#[derive(YaSerialize)]
struct Items {
  #[yaserde(attribute = true, prefix = "bar", rename = "count")]
  count: Option<String>,
  #[yaserde(rename = "Item")]
  entries: Vec<Item>,
}

#[derive(YaSerialize)]
struct Item {
  #[yaserde(attribute = true, prefix = "bar", rename = "kind")]
  kind: Option<String>,
  #[yaserde(rename = "Label")]
  label: String,
  #[yaserde(rename = "Value")]
  value: Option<Value>,
}

#[derive(YaSerialize)]
struct Value {
  #[yaserde(attribute = true, prefix = "bar", rename = "format")]
  format: Option<String>,
  #[yaserde(text = true)]
  content: Option<String>,
}

#[derive(Clone, Copy)]
enum Case {
  NoHook,
  Noop,
  Update,
}

impl Case {
  const ALL: [Self; 3] = [Self::NoHook, Self::Noop, Self::Update];

  fn name(self) -> &'static str {
    match self {
      Self::NoHook => "no hook",
      Self::Noop => "no-op hook",
      Self::Update => "foo v1 -> v2",
    }
  }

  fn index(self) -> usize {
    match self {
      Self::NoHook => 0,
      Self::Noop => 1,
      Self::Update => 2,
    }
  }
}

fn payload(size: usize) -> Foo {
  Foo {
    bar: Bar {
      version: FOO_V1.to_owned(),
      baz: Baz {
        items: Items {
          count: Some(size.to_string()),
          entries: (0..size)
            .map(|index| Item {
              kind: Some("example".to_owned()),
              label: format!("item-{index}"),
              value: Some(Value {
                format: Some("text".to_owned()),
                content: Some(format!("value-{index}")),
              }),
            })
            .collect(),
        },
      },
    },
  }
}

fn serialize(payload: &Foo, case: Case) -> String {
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  match case {
    Case::NoHook => {}
    Case::Noop => serializer.set_start_element_updater(|_| {}),
    Case::Update => serializer.set_start_element_updater(|event| {
      event.update_namespace("foo", FOO_V2);
    }),
  }
  payload.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

fn assert_foo_namespace(xml: &str) {
  let mut reader = XmlRsReader::from_reader(xml.as_bytes());
  loop {
    match reader.next_event().unwrap() {
      XmlReadEvent::StartElement { name, .. } if name.prefix_ref() == Some("foo") => {
        assert_eq!(name.namespace_ref(), Some(FOO_V2));
      }
      XmlReadEvent::EndDocument => break,
      _ => {}
    }
  }
}

fn median(samples: &mut [u128]) -> u128 {
  samples.sort_unstable();
  samples[samples.len() / 2]
}

fn parse_arg(args: &mut impl Iterator<Item = String>, default: usize) -> usize {
  args
    .next()
    .map(|value| {
      value
        .parse()
        .unwrap_or_else(|_| panic!("size and iterations must be integers"))
    })
    .unwrap_or(default)
}

fn main() {
  let mut args = env::args().skip(1);
  let size = parse_arg(&mut args, 2_000);
  let iterations = parse_arg(&mut args, 50);
  assert!(
    args.next().is_none(),
    "usage: start_element_updater_bench [size] [iterations]"
  );
  assert!(iterations > 0, "iterations must be positive");

  let payload = payload(size);
  let no_hook = serialize(&payload, Case::NoHook);
  assert_eq!(
    serialize(&payload, Case::Noop),
    no_hook,
    "no-op hook changed bytes"
  );
  let updated = serialize(&payload, Case::Update);
  assert!(updated.contains("xmlns:foo=\"urn:example:foo:v2\""));
  assert!(!updated.contains("xmlns:foo=\"urn:example:foo:v1\""));
  assert!(updated.contains(&format!("bar:version=\"{FOO_V1}\"")));
  assert_foo_namespace(&updated);
  assert!(updated.contains(BAR));
  assert!(updated.contains(&format!("bar:count=\"{size}\"")));
  assert!(updated.contains("bar:kind=\"example\""));
  assert!(updated.contains("bar:format=\"text\""));
  assert!(updated.contains("item-0"));
  assert!(updated.contains(&format!("value-{}", size.saturating_sub(1))));

  const WARMUPS: usize = 3;
  const SAMPLES: usize = 7;
  for round in 0..WARMUPS {
    for offset in 0..Case::ALL.len() {
      black_box(serialize(
        &payload,
        Case::ALL[(round + offset) % Case::ALL.len()],
      ));
    }
  }

  let mut samples = [Vec::new(), Vec::new(), Vec::new()];
  for round in 0..SAMPLES {
    for offset in 0..Case::ALL.len() {
      let case = Case::ALL[(round + offset) % Case::ALL.len()];
      let start = Instant::now();
      for _ in 0..iterations {
        black_box(serialize(&payload, case));
      }
      samples[case.index()].push(start.elapsed().as_nanos() / iterations as u128);
    }
  }

  let baseline = median(&mut samples[Case::NoHook.index()]);
  println!("size={size}, iterations={iterations}, warmups={WARMUPS}, samples={SAMPLES}");
  for case in Case::ALL {
    let ns_per_op = median(&mut samples[case.index()]);
    println!(
      "{:20} {:>10} ns/op  {:>5.2}x",
      case.name(),
      ns_per_op,
      ns_per_op as f64 / baseline as f64
    );
  }
}
