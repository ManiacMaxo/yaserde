extern crate yaserde_derive;

use std::borrow::Cow;
use std::io::Cursor;
use std::sync::{
  atomic::{AtomicUsize, Ordering},
  Arc,
};
use yaserde::ser::Serializer;
use yaserde::xml::{XmlAttribute, XmlName, XmlNamespace, XmlStartElement};
use yaserde::YaSerialize;
#[cfg(not(feature = "derive"))]
use yaserde_derive::YaSerialize;

const FOO_V1: &str = "urn:example:foo:v1";
const FOO_V2: &str = "urn:example:foo:v2";

#[derive(YaSerialize)]
#[yaserde(rename = "Foo", prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1" })]
struct Foo {
  #[yaserde(rename = "Bar", prefix = "foo")]
  bar: Bar,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Bar", prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1" })]
struct Bar {
  #[yaserde(attribute = true)]
  source: String,
  #[yaserde(rename = "Baz", prefix = "foo")]
  baz: Baz,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Baz", prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1" })]
struct Baz {
  #[yaserde(rename = "Items", prefix = "foo")]
  items: Items,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Items", prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1" })]
struct Items {
  #[yaserde(rename = "Item", prefix = "foo")]
  entries: Vec<Item>,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Item", prefix = "foo", namespaces = { "foo" = "urn:example:foo:v1" })]
struct Item {
  #[yaserde(rename = "Label", prefix = "foo")]
  label: String,
  #[yaserde(rename = "Value", prefix = "foo")]
  value: String,
}

fn model() -> Foo {
  Foo {
    bar: Bar {
      source: FOO_V1.to_owned(),
      baz: Baz {
        items: Items {
          entries: (0..2)
            .map(|index| Item {
              label: format!("item-{index}"),
              value: format!("kept-{FOO_V1}-{index}"),
            })
            .collect(),
        },
      },
    },
  }
}

fn serialize_with_updater<F>(model: &Foo, updater: F) -> String
where
  F: FnMut(&mut XmlStartElement) + Send + Sync + 'static,
{
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(updater);
  model.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

fn serialize_without_updater(model: &Foo) -> String {
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  model.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

#[test]
fn no_hook_preserves_derived_foo_output() {
  let xml = serialize_without_updater(&model());
  assert!(xml.starts_with("<foo:Foo xmlns:foo=\"urn:example:foo:v1\">"));
  assert!(xml.contains("<foo:Baz>"), "{}", xml);
  assert!(!xml.contains("<foo:Baz xmlns:"), "{}", xml);
}

#[test]
fn noop_updater_is_byte_equivalent_with_inherited_and_default_namespaces() {
  let derived = model();
  let expected = serialize_without_updater(&derived);
  assert_eq!(serialize_with_updater(&derived, |_| {}), expected);

  let mut namespace = XmlNamespace::empty();
  namespace.force_put("", "urn:example:default");
  namespace.force_put("p", FOO_V1);
  let mut unmodified = Serializer::new_for_inner(Cursor::new(Vec::new()));
  unmodified
    .write_start_element("root", Vec::new(), namespace.clone())
    .unwrap();
  unmodified
    .write_start_element("p:child", Vec::new(), XmlNamespace::empty())
    .unwrap();
  unmodified.write_end_element().unwrap();
  unmodified.write_end_element().unwrap();
  let expected = String::from_utf8(unmodified.into_inner().into_inner()).unwrap();

  let mut noop = Serializer::new_for_inner(Cursor::new(Vec::new()));
  noop.set_start_element_updater(|_| {});
  noop
    .write_start_element("root", Vec::new(), namespace)
    .unwrap();
  noop
    .write_start_element("p:child", Vec::new(), XmlNamespace::empty())
    .unwrap();
  noop.write_end_element().unwrap();
  noop.write_end_element().unwrap();
  assert_eq!(
    String::from_utf8(noop.into_inner().into_inner()).unwrap(),
    expected
  );
}

#[test]
fn updater_remaps_derived_foo_namespaces_without_changing_values() {
  let xml = serialize_with_updater(&model(), |event| {
    event.update_namespace("foo", FOO_V2);
  });
  assert!(xml.contains("xmlns:foo=\"urn:example:foo:v2\""));
  assert!(!xml.contains("xmlns:foo=\"urn:example:foo:v1\""));
  assert!(xml.contains(&format!("source=\"{FOO_V1}\"")));
  assert!(xml.contains(&format!("kept-{FOO_V1}")));
  assert!(xml.contains("<foo:Baz>"), "{}", xml);

  let mut reader = yaserde::xml::XmlRsReader::from_reader(xml.as_bytes());
  loop {
    match yaserde::xml::XmlEventReader::next_event(&mut reader).unwrap() {
      yaserde::xml::XmlReadEvent::StartElement { name, .. } => {
        if name.prefix_ref() == Some("foo") {
          assert_eq!(name.namespace_ref(), Some(FOO_V2));
        }
      }
      yaserde::xml::XmlReadEvent::EndDocument => break,
      _ => {}
    }
  }
}

#[test]
fn updater_remaps_actual_nested_old_binding() {
  let mut outer_namespace = XmlNamespace::empty();
  outer_namespace.force_put("f", "urn:example:outer");
  let mut nested_namespace = XmlNamespace::empty();
  nested_namespace.force_put("f", FOO_V1);
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(|event| {
    if event.namespace.0.get("f").map(String::as_str) == Some(FOO_V1) {
      event.update_namespace("f", FOO_V2);
    }
  });
  serializer
    .write_start_element("f:Foo", Vec::new(), outer_namespace)
    .unwrap();
  serializer
    .write_start_element("f:Bar", Vec::new(), nested_namespace)
    .unwrap();
  serializer.write_end_element().unwrap();
  serializer.write_end_element().unwrap();
  assert_eq!(
    String::from_utf8(serializer.into_inner().into_inner()).unwrap(),
    "<f:Foo xmlns:f=\"urn:example:outer\"><f:Bar xmlns:f=\"urn:example:foo:v2\" /></f:Foo>"
  );
}

#[test]
fn raw_writer_maps_explicitly_qualified_name_and_attributes_once() {
  let calls = Arc::new(AtomicUsize::new(0));
  let callback_calls = Arc::clone(&calls);
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(move |event| {
    callback_calls.fetch_add(1, Ordering::Relaxed);
    event.update_namespace("p", FOO_V2);
  });
  let namespace = ::xml::namespace::Namespace(
    vec![("p".to_owned(), FOO_V1.to_owned())]
      .into_iter()
      .collect(),
  );
  serializer
    .write(::xml::writer::events::XmlEvent::StartElement {
      name: ::xml::name::Name::qualified("raw", FOO_V1, Some("p")),
      attributes: Cow::Owned(vec![::xml::attribute::Attribute::new(
        ::xml::name::Name::qualified("id", FOO_V1, Some("p")),
        "42",
      )]),
      namespace: Cow::Owned(namespace),
    })
    .unwrap();
  serializer
    .write(::xml::writer::XmlEvent::end_element())
    .unwrap();
  assert_eq!(calls.load(Ordering::Relaxed), 1);
  assert_eq!(
    String::from_utf8(serializer.into_inner().into_inner()).unwrap(),
    "<p:raw xmlns:p=\"urn:example:foo:v2\" p:id=\"42\" />"
  );
}

#[test]
fn clearing_updater_stops_callbacks() {
  let calls = Arc::new(AtomicUsize::new(0));
  let callback_calls = Arc::clone(&calls);
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(move |_| {
    callback_calls.fetch_add(1, Ordering::Relaxed);
  });
  serializer
    .write_start_element("first", Vec::new(), XmlNamespace::empty())
    .unwrap();
  serializer.write_end_element().unwrap();
  serializer.clear_start_element_updater();
  serializer
    .write_start_element("second", Vec::new(), XmlNamespace::empty())
    .unwrap();
  serializer.write_end_element().unwrap();
  assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn derived_serialization_calls_updater_once_per_start_element() {
  let calls = Arc::new(AtomicUsize::new(0));
  let callback_calls = Arc::clone(&calls);
  serialize_with_updater(&model(), move |_| {
    callback_calls.fetch_add(1, Ordering::Relaxed);
  });
  assert_eq!(calls.load(Ordering::Relaxed), 10);
}

#[test]
fn serializer_remains_send_and_sync() {
  fn assert_send_sync<T: Send + Sync>() {}

  assert_send_sync::<Serializer<Vec<u8>>>();
}

#[test]
fn namespace_helpers_keep_expanded_names_consistent() {
  let mut event = XmlStartElement::new(
    XmlName::qualified("root", FOO_V1, Some("p")),
    vec![
      XmlAttribute::new(XmlName::qualified("id", FOO_V1, Some("p")), "1"),
      XmlAttribute::new(XmlName::local("plain"), "2"),
    ],
    XmlNamespace::empty(),
  );
  assert!(!event.update_namespace("p", FOO_V2));
  assert_eq!(event.set_namespace("p", FOO_V2), None);
  assert_eq!(event.name.namespace_ref(), Some(FOO_V2));
  assert_eq!(event.attributes[0].name.namespace_ref(), Some(FOO_V2));
  event.set_namespace("", "urn:example:default");
  assert_eq!(event.attributes[1].name.namespace_ref(), None);
  event.set_attribute("plain", "changed");
  event.set_attribute("added", "3");
  assert_eq!(event.attributes.len(), 3);
  assert_eq!(event.attributes[1].value, "changed");
}

#[test]
fn serializers_keep_owned_callback_configuration_independent() {
  let first_uri = String::from("urn:example:first");
  let first = serialize_with_updater(&model(), move |event| {
    event.update_namespace("foo", first_uri.clone());
  });
  let second_uri = String::from("urn:example:second");
  let second = serialize_with_updater(&model(), move |event| {
    event.update_namespace("foo", second_uri.clone());
  });
  assert!(first.contains("xmlns:foo=\"urn:example:first\""));
  assert!(second.contains("xmlns:foo=\"urn:example:second\""));
}
