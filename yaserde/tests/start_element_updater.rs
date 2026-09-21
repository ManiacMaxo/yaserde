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

const CWMP_10: &str = "urn:dslforum-org:cwmp-1-0";
const CWMP_13: &str = "urn:dslforum-org:cwmp-1-3";

#[derive(YaSerialize)]
#[yaserde(rename = "Envelope", prefix = "soap-env", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct Envelope {
  #[yaserde(rename = "Header", prefix = "soap-env")]
  header: Header,
  #[yaserde(rename = "Body", prefix = "soap-env")]
  body: Body,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Header", prefix = "soap-env", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct Header {
  #[yaserde(attribute = true)]
  source: String,
  #[yaserde(rename = "ID", prefix = "cwmp")]
  id: String,
}

#[derive(YaSerialize)]
#[yaserde(rename = "Body", prefix = "soap-env", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct Body {
  #[yaserde(rename = "GetParameterValuesResponse", prefix = "cwmp")]
  response: GetParameterValuesResponse,
}

#[derive(YaSerialize)]
#[yaserde(rename = "GetParameterValuesResponse", prefix = "cwmp", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct GetParameterValuesResponse {
  #[yaserde(rename = "ParameterList", prefix = "cwmp")]
  parameter_list: ParameterList,
}

#[derive(YaSerialize)]
#[yaserde(rename = "ParameterList", prefix = "cwmp", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct ParameterList {
  #[yaserde(rename = "ParameterValueStruct", prefix = "cwmp")]
  parameters: Vec<ParameterValueStruct>,
}

#[derive(YaSerialize)]
#[yaserde(rename = "ParameterValueStruct", prefix = "cwmp", namespaces = { "soap-env" = "http://schemas.xmlsoap.org/soap/envelope/", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct ParameterValueStruct {
  #[yaserde(rename = "Name", prefix = "cwmp")]
  name: String,
  #[yaserde(rename = "Value", prefix = "cwmp")]
  value: String,
}

fn model() -> Envelope {
  Envelope {
    header: Header {
      source: CWMP_10.to_owned(),
      id: format!("kept-{CWMP_10}"),
    },
    body: Body {
      response: GetParameterValuesResponse {
        parameter_list: ParameterList {
          parameters: (0..2)
            .map(|index| ParameterValueStruct {
              name: format!("Device.WiFi.SSID.{index}.SSID"),
              value: format!("kept-{CWMP_10}-{index}"),
            })
            .collect(),
        },
      },
    },
  }
}

fn serialize_with_updater<F>(model: &Envelope, updater: F) -> String
where
  F: FnMut(&mut XmlStartElement) + Send + Sync + 'static,
{
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(updater);
  model.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

fn serialize_without_updater(model: &Envelope) -> String {
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  model.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

#[test]
fn no_hook_preserves_derived_soap_output() {
  let xml = serialize_without_updater(&model());
  assert!(xml.starts_with("<soap-env:Envelope xmlns:cwmp=\"urn:dslforum-org:cwmp-1-0\" xmlns:soap-env=\"http://schemas.xmlsoap.org/soap/envelope/\">"));
  assert!(xml.contains("<soap-env:Body>"), "{}", xml);
  assert!(!xml.contains("<soap-env:Body xmlns:"), "{}", xml);
}

#[test]
fn noop_updater_is_byte_equivalent_with_inherited_and_default_namespaces() {
  let derived = model();
  let expected = serialize_without_updater(&derived);
  assert_eq!(serialize_with_updater(&derived, |_| {}), expected);

  let mut namespace = XmlNamespace::empty();
  namespace.force_put("", "urn:default");
  namespace.force_put("p", CWMP_10);
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
fn updater_remaps_derived_cwmp_namespaces_without_changing_values() {
  let xml = serialize_with_updater(&model(), |event| {
    event.update_namespace("cwmp", CWMP_13);
  });
  assert!(xml.contains("xmlns:cwmp=\"urn:dslforum-org:cwmp-1-3\""));
  assert!(!xml.contains("xmlns:cwmp=\"urn:dslforum-org:cwmp-1-0\""));
  assert!(xml.contains(&format!("source=\"{CWMP_10}\"")));
  assert!(xml.contains(&format!("kept-{CWMP_10}")));
  assert!(xml.contains("<soap-env:Body>"), "{}", xml);

  let mut reader = yaserde::xml::XmlRsReader::from_reader(xml.as_bytes());
  loop {
    match yaserde::xml::XmlEventReader::next_event(&mut reader).unwrap() {
      yaserde::xml::XmlReadEvent::StartElement { name, .. } => {
        if name.prefix_ref() == Some("cwmp") {
          assert_eq!(name.namespace_ref(), Some(CWMP_13));
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
  outer_namespace.force_put("s", "urn:outer");
  let mut nested_namespace = XmlNamespace::empty();
  nested_namespace.force_put("s", CWMP_10);
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(|event| {
    if event.namespace.0.get("s").map(String::as_str) == Some(CWMP_10) {
      event.update_namespace("s", CWMP_13);
    }
  });
  serializer
    .write_start_element("s:Envelope", Vec::new(), outer_namespace)
    .unwrap();
  serializer
    .write_start_element("s:Body", Vec::new(), nested_namespace)
    .unwrap();
  serializer.write_end_element().unwrap();
  serializer.write_end_element().unwrap();
  assert_eq!(
    String::from_utf8(serializer.into_inner().into_inner()).unwrap(),
    "<s:Envelope xmlns:s=\"urn:outer\"><s:Body xmlns:s=\"urn:dslforum-org:cwmp-1-3\" /></s:Envelope>"
  );
}

#[test]
fn raw_writer_maps_explicitly_qualified_name_and_attributes_once() {
  let calls = Arc::new(AtomicUsize::new(0));
  let callback_calls = Arc::clone(&calls);
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  serializer.set_start_element_updater(move |event| {
    callback_calls.fetch_add(1, Ordering::Relaxed);
    event.update_namespace("p", CWMP_13);
  });
  let namespace = ::xml::namespace::Namespace(
    vec![("p".to_owned(), CWMP_10.to_owned())]
      .into_iter()
      .collect(),
  );
  serializer
    .write(::xml::writer::events::XmlEvent::StartElement {
      name: ::xml::name::Name::qualified("raw", CWMP_10, Some("p")),
      attributes: Cow::Owned(vec![::xml::attribute::Attribute::new(
        ::xml::name::Name::qualified("id", CWMP_10, Some("p")),
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
    "<p:raw xmlns:p=\"urn:dslforum-org:cwmp-1-3\" p:id=\"42\" />"
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
  assert_eq!(calls.load(Ordering::Relaxed), 12);
}

#[test]
fn serializer_remains_send_and_sync() {
  fn assert_send_sync<T: Send + Sync>() {}

  assert_send_sync::<Serializer<Vec<u8>>>();
}

#[test]
fn namespace_helpers_keep_expanded_names_consistent() {
  let mut event = XmlStartElement::new(
    XmlName::qualified("root", CWMP_10, Some("p")),
    vec![
      XmlAttribute::new(XmlName::qualified("id", CWMP_10, Some("p")), "1"),
      XmlAttribute::new(XmlName::local("plain"), "2"),
    ],
    XmlNamespace::empty(),
  );
  assert!(!event.update_namespace("p", CWMP_13));
  assert_eq!(event.set_namespace("p", CWMP_13), None);
  assert_eq!(event.name.namespace_ref(), Some(CWMP_13));
  assert_eq!(event.attributes[0].name.namespace_ref(), Some(CWMP_13));
  event.set_namespace("", "urn:default");
  assert_eq!(event.attributes[1].name.namespace_ref(), None);
  event.set_attribute("plain", "changed");
  event.set_attribute("added", "3");
  assert_eq!(event.attributes.len(), 3);
  assert_eq!(event.attributes[1].value, "changed");
}

#[test]
fn serializers_keep_owned_callback_configuration_independent() {
  let first_uri = String::from("urn:first");
  let first = serialize_with_updater(&model(), move |event| {
    event.update_namespace("cwmp", first_uri.clone());
  });
  let second_uri = String::from("urn:second");
  let second = serialize_with_updater(&model(), move |event| {
    event.update_namespace("cwmp", second_uri.clone());
  });
  assert!(first.contains("xmlns:cwmp=\"urn:first\""));
  assert!(second.contains("xmlns:cwmp=\"urn:second\""));
}
