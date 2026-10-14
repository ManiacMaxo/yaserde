#[macro_use]
extern crate yaserde_derive;

use yaserde::de::{from_reader_dyn, from_reader_with_parser};
use yaserde::ser::{serialize_with_emitter, Config};
#[cfg(feature = "quick-xml-backend")]
use yaserde::xml::{XmlAttribute, XmlName, XmlNamespace};
use yaserde::xml::{XmlEventWriter, XmlRsReader, XmlRsWriter, XmlWriteEvent};

#[derive(Default)]
struct RecordingWriter(Vec<String>);

impl XmlEventWriter for RecordingWriter {
  fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    self.0.push(format!("{event:?}"));
    Ok(())
  }
}

#[derive(Debug, PartialEq, YaDeserialize, YaSerialize)]
struct Root {
  item: String,
}

const XML: &str = "<Root><item>a<![CDATA[b]]>c</item></Root>";

#[test]
fn xml_rs_backend_can_be_selected_explicitly() {
  let parser = XmlRsReader::from_reader(XML.as_bytes());
  let loaded: Root = from_reader_with_parser(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[test]
fn xml_rs_backend_can_be_selected_dynamically() {
  let parser = Box::new(XmlRsReader::from_reader(XML.as_bytes()));
  let loaded: Root = from_reader_dyn(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[test]
fn serializers_accept_recording_boxed_and_borrowed_emitters() {
  let model = Root {
    item: "value".into(),
  };
  let recorder = serialize_with_emitter(&model, RecordingWriter::default()).unwrap();
  assert_eq!(recorder.0.len(), 5);

  let mut borrowed = RecordingWriter::default();
  serialize_with_emitter(&model, &mut borrowed).unwrap();
  assert_eq!(borrowed.0.len(), 5);

  let boxed: Box<dyn XmlEventWriter> = Box::new(RecordingWriter::default());
  serialize_with_emitter(&model, boxed).unwrap();
}

#[test]
fn xml_rs_writer_round_trips_with_xml_rs_reader() {
  let model = Root { item: "<&>".into() };
  let emitter = XmlRsWriter::from_writer(Vec::new(), &Config::default());
  let xml = String::from_utf8(
    serialize_with_emitter(&model, emitter)
      .unwrap()
      .into_inner(),
  )
  .unwrap();
  let loaded: Root = from_reader_with_parser(XmlRsReader::from_reader(xml.as_bytes())).unwrap();
  assert_eq!(loaded, model);
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_backend_can_be_selected_explicitly() {
  let parser = yaserde::xml::QuickXmlReader::from_reader(std::io::Cursor::new(XML));
  let loaded: Root = from_reader_with_parser(parser).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_writer_round_trips_with_both_readers_and_honors_indent() {
  use yaserde::xml::QuickXmlWriter;

  let model = Root {
    item: "value".into(),
  };
  let config = Config {
    perform_indent: true,
    write_document_declaration: false,
    indent_string: Some("\t".into()),
  };
  let emitter = QuickXmlWriter::from_writer(Vec::new(), &config);
  let xml = String::from_utf8(
    serialize_with_emitter(&model, emitter)
      .unwrap()
      .into_inner(),
  )
  .unwrap();
  assert!(xml.contains("\n\t<item>"));
  let custom = Config {
    indent_string: Some("--".into()),
    ..config
  };
  let custom_xml = serialize_with_emitter(&model, QuickXmlWriter::from_writer(Vec::new(), &custom))
    .unwrap()
    .into_inner();
  assert!(String::from_utf8(custom_xml)
    .unwrap()
    .contains("\n--<item>"));
  let escaped = serialize_with_emitter(
    &Root { item: "<&>".into() },
    QuickXmlWriter::from_writer(Vec::new(), &Config::default()),
  )
  .unwrap()
  .into_inner();
  assert!(String::from_utf8(escaped)
    .unwrap()
    .contains("&lt;&amp;&gt;"));

  let mut writer = QuickXmlWriter::from_writer(
    Vec::new(),
    &Config {
      write_document_declaration: false,
      ..Config::default()
    },
  );
  let mut namespace = XmlNamespace::empty();
  namespace.put("ns", "urn:example?x=\"y\"");
  writer
    .write_event(XmlWriteEvent::StartElement {
      name: "ns:root".into(),
      attributes: vec![XmlAttribute::new(XmlName::local("value"), "<&\"'")],
      namespace,
    })
    .unwrap();
  writer
    .write_event(XmlWriteEvent::CData("raw <& text".into()))
    .unwrap();
  writer.write_event(XmlWriteEvent::EndElement).unwrap();
  assert_eq!(
    String::from_utf8(writer.into_inner()).unwrap(),
    "<ns:root xmlns:ns=\"urn:example?x=&quot;y&quot;\" value=\"&lt;&amp;&quot;&apos;\"><![CDATA[raw <& text]]></ns:root>"
  );

  let mut errors = QuickXmlWriter::from_writer(Vec::new(), &Config::default());
  assert!(errors.write_event(XmlWriteEvent::EndElement).is_err());
  let xml_rs: Root = from_reader_with_parser(XmlRsReader::from_reader(xml.as_bytes())).unwrap();
  let quick: Root = from_reader_with_parser(yaserde::xml::QuickXmlReader::from_reader(
    std::io::Cursor::new(&xml),
  ))
  .unwrap();
  assert_eq!(xml_rs, model);
  assert_eq!(quick, model);
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_attributes_preserve_whitespace_and_escaping() {
  #[derive(YaSerialize)]
  struct Attributed {
    #[yaserde(attribute = true)]
    value: String,
  }

  for value in ["plain", "a\tb\nc\rd", "<&\"'>&amp;", "é\n水"] {
    let model = Attributed {
      value: value.into(),
    };
    let bytes = serialize_with_emitter(&model, yaserde::xml::QuickXmlWriter::new(Vec::new()))
      .unwrap()
      .into_inner();
    let xml = String::from_utf8(bytes).unwrap();
    // An independent parser applies XML attribute-value normalization.
    let parsed = roxmltree::Document::parse(&xml).unwrap();
    assert_eq!(parsed.root_element().attribute("value"), Some(value));
  }
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_cdata_preserves_closing_delimiters() {
  for value in ["", "raw <& text", "a]]>b", "]]>]]>", "é]]>水"] {
    let mut writer = yaserde::xml::QuickXmlWriter::new(Vec::new());
    writer
      .write_event(XmlWriteEvent::StartElement {
        name: "root".into(),
        attributes: Vec::new(),
        namespace: XmlNamespace::empty(),
      })
      .unwrap();
    writer
      .write_event(XmlWriteEvent::CData(value.into()))
      .unwrap();
    writer.write_event(XmlWriteEvent::EndElement).unwrap();
    let xml = String::from_utf8(writer.into_inner()).unwrap();
    let parsed = roxmltree::Document::parse(&xml).unwrap();
    let text: String = parsed
      .root_element()
      .children()
      .filter_map(|node| node.text())
      .collect();
    assert_eq!(text, value);
  }
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_restores_namespace_bindings_after_each_element() {
  fn start(writer: &mut impl XmlEventWriter, name: &str, bindings: &[(&str, &str)]) {
    let mut namespace = XmlNamespace::empty();
    for &(prefix, uri) in bindings {
      namespace.put(prefix, uri);
    }
    writer
      .write_event(XmlWriteEvent::StartElement {
        name: name.into(),
        attributes: Vec::new(),
        namespace,
      })
      .unwrap();
  }

  let mut writer = yaserde::xml::QuickXmlWriter::new(Vec::new());
  start(
    &mut writer,
    "root",
    &[("", "urn:default"), ("p", "urn:outer")],
  );
  // Exercise both deferred (empty) and flushed start tags.
  for has_child in [false, true] {
    start(
      &mut writer,
      "p:inner",
      &[("", ""), ("p", "urn:inner"), ("q", "urn:local")],
    );
    if has_child {
      start(&mut writer, "child", &[]);
      writer.write_event(XmlWriteEvent::EndElement).unwrap();
    }
    writer.write_event(XmlWriteEvent::EndElement).unwrap();
    start(&mut writer, "p:sibling", &[("p", "urn:outer")]);
    writer.write_event(XmlWriteEvent::EndElement).unwrap();
    start(&mut writer, "sibling", &[]);
    writer.write_event(XmlWriteEvent::EndElement).unwrap();
    // q must be declared again after the previous local binding left scope.
    start(&mut writer, "q:sibling", &[("q", "urn:local")]);
    writer.write_event(XmlWriteEvent::EndElement).unwrap();
  }
  writer.write_event(XmlWriteEvent::EndElement).unwrap();

  let xml = String::from_utf8(writer.into_inner()).unwrap();
  let parsed = roxmltree::Document::parse(&xml).unwrap();
  let names: Vec<_> = parsed
    .descendants()
    .filter(|node| node.is_element())
    .map(|node| {
      (
        node.tag_name().name(),
        node.tag_name().namespace().filter(|uri| !uri.is_empty()),
      )
    })
    .collect();
  assert_eq!(
    names,
    vec![
      ("root", Some("urn:default")),
      ("inner", Some("urn:inner")),
      ("sibling", Some("urn:outer")),
      ("sibling", Some("urn:default")),
      ("sibling", Some("urn:local")),
      ("inner", Some("urn:inner")),
      ("child", None),
      ("sibling", Some("urn:outer")),
      ("sibling", Some("urn:default")),
      ("sibling", Some("urn:local")),
    ]
  );
}

#[derive(Debug, PartialEq, YaDeserialize)]
#[yaserde(
  rename = "Envelope",
  namespaces = {
    "s" = "urn:test",
  },
  prefix = "s"
)]
struct NamespacedEnvelope {
  #[yaserde(attribute = true)]
  id: String,
  #[yaserde(rename = "child", prefix = "s")]
  child: String,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct FlattenOuter {
  known: String,
  #[yaserde(flatten = true)]
  extra: FlattenExtra,
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct FlattenExtra {
  other: String,
}

#[derive(Debug, PartialEq, YaDeserialize)]
enum Choice {
  A,
  B,
}

impl Default for Choice {
  fn default() -> Self {
    Choice::A
  }
}

#[derive(Debug, PartialEq, YaDeserialize)]
struct ChoiceHolder {
  choice: Choice,
}

fn assert_xml_rs_parity_cases() {
  let spaced = "<Root><!-- skip --><item> a <![CDATA[b]]> c </item></Root>";
  let loaded: Root = from_reader_with_parser(XmlRsReader::from_reader(spaced.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    Root {
      item: "a b c".into()
    }
  );

  let namespaced = r#"<s:Envelope xmlns:s="urn:test" id="1"><s:child>value</s:child></s:Envelope>"#;
  let loaded: NamespacedEnvelope =
    from_reader_with_parser(XmlRsReader::from_reader(namespaced.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    NamespacedEnvelope {
      id: "1".into(),
      child: "value".into(),
    }
  );

  let flattened = "<FlattenOuter><known>k</known><other>o</other></FlattenOuter>";
  let loaded: FlattenOuter =
    from_reader_with_parser(XmlRsReader::from_reader(flattened.as_bytes())).unwrap();
  assert_eq!(
    loaded,
    FlattenOuter {
      known: "k".into(),
      extra: FlattenExtra { other: "o".into() },
    }
  );

  let choice = "<ChoiceHolder><choice>B</choice></ChoiceHolder>";
  let loaded: ChoiceHolder =
    from_reader_with_parser(XmlRsReader::from_reader(choice.as_bytes())).unwrap();
  assert_eq!(loaded, ChoiceHolder { choice: Choice::B });
}

#[test]
fn xml_rs_backend_parity_cases() {
  assert_xml_rs_parity_cases();
}

#[cfg(feature = "quick-xml-backend")]
#[test]
fn quick_xml_backend_parity_cases() {
  use yaserde::xml::QuickXmlReader;

  let spaced = "<Root><!-- skip --><item> a <![CDATA[b]]> c </item></Root>";
  let loaded: Root =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(spaced))).unwrap();
  assert_eq!(loaded, Root { item: "abc".into() });

  let namespaced = r#"<s:Envelope xmlns:s="urn:test" id="1"><s:child>value</s:child></s:Envelope>"#;
  let loaded: NamespacedEnvelope = from_reader_with_parser(QuickXmlReader::from_reader(
    std::io::Cursor::new(namespaced),
  ))
  .unwrap();
  assert_eq!(
    loaded,
    NamespacedEnvelope {
      id: "1".into(),
      child: "value".into(),
    }
  );

  let flattened = "<FlattenOuter><known>k</known><other>o</other></FlattenOuter>";
  let loaded: FlattenOuter =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(flattened))).unwrap();
  assert_eq!(
    loaded,
    FlattenOuter {
      known: "k".into(),
      extra: FlattenExtra { other: "o".into() },
    }
  );

  let choice = "<ChoiceHolder><choice>B</choice></ChoiceHolder>";
  let loaded: ChoiceHolder =
    from_reader_with_parser(QuickXmlReader::from_reader(std::io::Cursor::new(choice))).unwrap();
  assert_eq!(loaded, ChoiceHolder { choice: Choice::B });
}
