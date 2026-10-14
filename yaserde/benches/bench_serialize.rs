use criterion::{
  black_box, criterion_group, criterion_main, measurement::WallTime, BenchmarkGroup, Criterion,
};
use std::io::Cursor;
use yaserde::ser::{serialize_with_emitter, Config};
use yaserde::xml::{QuickXmlWriter, XmlRsWriter};

#[macro_use]
extern crate yaserde_derive;

// ── FastXML address-book schema (serialize + deserialize for round-trip) ─

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Database {
  person: Vec<Person>,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Person {
  #[yaserde(attribute = true)]
  id: String,
  name: String,
  email: String,
  phone: String,
  address: Address,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Address {
  #[yaserde(attribute = true)]
  city: String,
  #[yaserde(attribute = true)]
  state: String,
  #[yaserde(attribute = true)]
  zip: String,
  #[yaserde(attribute = true)]
  country: String,
  line1: String,
  line2: String,
}

// ── Synthetic types ───────────────────────────────────────────────────────

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Simple {
  #[yaserde(attribute = true)]
  id: i32,
  #[yaserde(attribute = true)]
  active: bool,
  name: String,
  score: f64,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Nested {
  #[yaserde(attribute = true)]
  version: String,
  header: Header,
  body: Body,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Header {
  timestamp: u64,
  sender: String,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Body {
  items: Vec<Item>,
}

#[derive(Debug, PartialEq, YaSerialize, YaDeserialize)]
struct Item {
  #[yaserde(attribute = true)]
  index: u32,
  value: String,
}

#[derive(YaSerialize)]
#[yaserde(
    rename = "Envelope",
    namespaces = {
        "s" = "urn:test",
        "a" = "urn:a", "b" = "urn:b", "c" = "urn:c",
        "d" = "urn:d", "e" = "urn:e", "f" = "urn:f", "g" = "urn:g",
    },
    prefix = "s"
)]
struct NamespacedEnvelope {
  #[yaserde(rename = "child", prefix = "s")]
  child: Vec<String>,
}

// ── FastXML data ──────────────────────────────────────────────────────────

const ADDRESS_SMALL: &str = include_str!("data/address-small.xml");
const ADDRESS_MIDDLE: &str = include_str!("data/address-middle.xml");

fn address_big() -> &'static String {
  use std::sync::OnceLock;
  static DATA: OnceLock<String> = OnceLock::new();
  DATA.get_or_init(|| {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(dir.join("benches/data/address-big.xml"))
      .expect("failed to read address-big.xml")
  })
}

// ── Deserialize helpers ───────────────────────────────────────────────────

fn de_xml_rs<T: yaserde::YaDeserialize>(xml: &str) -> T {
  let parser = yaserde::xml::XmlRsReader::from_reader(xml.as_bytes());
  yaserde::de::from_reader_with_parser(parser).unwrap()
}

fn de_quick_xml<T: yaserde::YaDeserialize>(xml: &str) -> T {
  let parser = yaserde::xml::QuickXmlReader::from_reader(Cursor::new(xml));
  yaserde::de::from_reader_with_parser(parser).unwrap()
}

// ── Explicit writer selection keeps labels independent of feature defaults ──

fn ser_xml_rs<T: yaserde::YaSerialize>(model: &T) -> Vec<u8> {
  serialize_with_emitter(
    model,
    XmlRsWriter::from_writer(Vec::new(), &Config::default()),
  )
  .unwrap()
  .into_inner()
}

fn ser_quick_xml<T: yaserde::YaSerialize>(model: &T) -> Vec<u8> {
  serialize_with_emitter(model, QuickXmlWriter::new(Vec::new()))
    .unwrap()
    .into_inner()
}

fn bench_writers<T: yaserde::YaSerialize>(group: &mut BenchmarkGroup<'_, WallTime>, model: &T) {
  group.bench_function("xml-rs-writer", |b| b.iter(|| ser_xml_rs(black_box(model))));
  group.bench_function("quick-xml-writer", |b| {
    b.iter(|| ser_quick_xml(black_box(model)))
  });
}

fn bench_serialize_simple(c: &mut Criterion) {
  let model = Simple {
    id: 42,
    active: true,
    name: "Alice".into(),
    score: 98.6,
  };
  let mut group = c.benchmark_group("serialize/simple_struct");
  bench_writers(&mut group, &model);
  group.finish();
}

fn bench_serialize_nested(c: &mut Criterion) {
  let model = Nested {
    version: "1.0".into(),
    header: Header {
      timestamp: 1700000000,
      sender: "bench".into(),
    },
    body: Body {
      items: vec![
        Item {
          index: 0,
          value: "zero".into(),
        },
        Item {
          index: 1,
          value: "one".into(),
        },
        Item {
          index: 2,
          value: "two".into(),
        },
      ],
    },
  };
  let mut group = c.benchmark_group("serialize/nested_struct");
  bench_writers(&mut group, &model);
  group.finish();
}

fn bench_serialize_address_small(c: &mut Criterion) {
  let db: Database = de_quick_xml(ADDRESS_SMALL);
  let mut group = c.benchmark_group("serialize/address-small");
  group.sample_size(50);
  bench_writers(&mut group, &db);
  group.finish();
}

fn bench_serialize_address_middle(c: &mut Criterion) {
  let db: Database = de_quick_xml(ADDRESS_MIDDLE);
  let mut group = c.benchmark_group("serialize/address-middle");
  group.sample_size(20);
  bench_writers(&mut group, &db);
  group.finish();
}

fn bench_serialize_namespaced(c: &mut Criterion) {
  let model = NamespacedEnvelope {
    child: vec!["value".into(); 1000],
  };
  let mut group = c.benchmark_group("serialize/namespaced_1000_items");
  bench_writers(&mut group, &model);
  group.finish();
}

// ── Round-trip benchmarks ─────────────────────────────────────────────────

fn bench_roundtrip_address_small(c: &mut Criterion) {
  let db: Database = de_quick_xml(ADDRESS_SMALL);
  let mut group = c.benchmark_group("roundtrip/address-small");
  group.sample_size(30);
  group.bench_function("xml-rs", |b| {
    b.iter(|| {
      let xml = ser_xml_rs(black_box(&db));
      let _: Database = de_xml_rs(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.bench_function("quick-xml", |b| {
    b.iter(|| {
      let xml = ser_quick_xml(black_box(&db));
      let _: Database = de_quick_xml(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.finish();
}

fn bench_roundtrip_address_middle(c: &mut Criterion) {
  let db: Database = de_quick_xml(ADDRESS_MIDDLE);
  let mut group = c.benchmark_group("roundtrip/address-middle");
  group.sample_size(10);
  group.bench_function("xml-rs", |b| {
    b.iter(|| {
      let xml = ser_xml_rs(black_box(&db));
      let _: Database = de_xml_rs(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.bench_function("quick-xml", |b| {
    b.iter(|| {
      let xml = ser_quick_xml(black_box(&db));
      let _: Database = de_quick_xml(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.finish();
}

fn bench_roundtrip_address_big(c: &mut Criterion) {
  let db: Database = de_quick_xml(address_big());
  let mut group = c.benchmark_group("roundtrip/address-big");
  group.sample_size(10);
  group.measurement_time(std::time::Duration::from_secs(60));
  group.bench_function("xml-rs", |b| {
    b.iter(|| {
      let xml = ser_xml_rs(black_box(&db));
      let _: Database = de_xml_rs(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.bench_function("quick-xml", |b| {
    b.iter(|| {
      let xml = ser_quick_xml(black_box(&db));
      let _: Database = de_quick_xml(std::str::from_utf8(&xml).unwrap());
    })
  });
  group.finish();
}

// ── Criterion entry point ─────────────────────────────────────────────────

criterion_group!(
  serialize_benches,
  bench_serialize_simple,
  bench_serialize_nested,
  bench_serialize_address_small,
  bench_serialize_address_middle,
  bench_serialize_namespaced,
);
criterion_group!(
  roundtrip_benches,
  bench_roundtrip_address_small,
  bench_roundtrip_address_middle,
  bench_roundtrip_address_big,
);
criterion_main!(serialize_benches, roundtrip_benches);
