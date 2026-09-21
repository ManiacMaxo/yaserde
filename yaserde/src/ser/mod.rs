//! Generic XML serialization helpers.

use crate::xml::{XmlAttribute, XmlEventWriter, XmlNamespace, XmlRsWriter, XmlWriteEvent};
use crate::YaSerialize;
use ::xml::writer::XmlEvent;
use std::io::{Cursor, Write};
use std::str;

pub struct Config {
  pub perform_indent: bool,
  pub write_document_declaration: bool,
  pub indent_string: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      perform_indent: false,
      write_document_declaration: true,
      indent_string: None,
    }
  }
}

/// Serialize XML into a plain String with no formatting.
pub fn to_string<T: YaSerialize>(model: &T) -> Result<String, String> {
  to_string_with_config(model, &Config::default())
}

/// Serialize XML into a plain String with formatting controlled by `Config`.
pub fn to_string_with_config<T: YaSerialize>(model: &T, config: &Config) -> Result<String, String> {
  let cursor = serialize_with_writer(model, Cursor::new(Vec::new()), config)?;
  Ok(
    str::from_utf8(cursor.get_ref())
      .expect("Found invalid UTF-8")
      .into(),
  )
}

pub fn serialize_with_writer<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
  config: &Config,
) -> Result<W, String> {
  #[cfg(feature = "quick-xml-backend")]
  let writer = crate::xml::QuickXmlWriter::from_writer(writer, config);
  #[cfg(not(feature = "quick-xml-backend"))]
  let writer = XmlRsWriter::from_writer(writer, config);
  serialize_with_emitter(model, writer).map(|writer| writer.into_inner())
}

/// Serialize with an explicitly selected XML event emitter, returning that emitter.
pub fn serialize_with_emitter<E: XmlEventWriter, T: YaSerialize>(
  model: &T,
  emitter: E,
) -> Result<E, String> {
  let mut serializer = Serializer::new(emitter);
  model.serialize(&mut serializer)?;
  Ok(serializer.into_inner())
}

pub fn to_string_content<T: YaSerialize>(model: &T) -> Result<String, String> {
  let cursor = serialize_with_writer_content(model, Cursor::new(Vec::new()))?;
  Ok(
    str::from_utf8(cursor.get_ref())
      .expect("Found invalid UTF-8")
      .into(),
  )
}

pub fn serialize_with_writer_content<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
) -> Result<W, String> {
  let config = Config {
    write_document_declaration: false,
    ..Config::default()
  };
  #[cfg(feature = "quick-xml-backend")]
  let writer = crate::xml::QuickXmlWriter::from_writer(writer, &config);
  #[cfg(not(feature = "quick-xml-backend"))]
  let writer = XmlRsWriter::from_writer(writer, &config);
  let mut serializer = Serializer::new(writer);
  serializer.set_skip_start_end(true);
  model.serialize(&mut serializer)?;
  Ok(serializer.into_inner().into_inner())
}

/// Serializer state shared by all XML event writer backends.
pub struct Serializer<E: XmlEventWriter> {
  writer: E,
  skip_start_end: bool,
  start_event_name: Option<String>,
}

impl<E: XmlEventWriter> Serializer<E> {
  pub fn new(writer: E) -> Self {
    Self {
      writer,
      skip_start_end: false,
      start_event_name: None,
    }
  }

  pub fn into_inner(self) -> E {
    self.writer
  }
  pub fn skip_start_end(&self) -> bool {
    self.skip_start_end
  }
  pub fn set_skip_start_end(&mut self, state: bool) {
    self.skip_start_end = state;
  }
  pub fn get_start_event_name(&self) -> Option<String> {
    self.start_event_name.clone()
  }
  pub fn set_start_event_name(&mut self, name: Option<String>) {
    self.start_event_name = name;
  }
  pub fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    self.writer.write_event(event)
  }
  pub fn write_start_element<S: Into<String>>(
    &mut self,
    name: S,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> Result<(), String> {
    self.write_event(XmlWriteEvent::StartElement {
      name: name.into().into(),
      attributes,
      namespace,
    })
  }
  pub fn write_end_element(&mut self) -> Result<(), String> {
    self.write_event(XmlWriteEvent::EndElement)
  }
  pub fn write_characters(&mut self, text: &str) -> Result<(), String> {
    self.write_event(XmlWriteEvent::Characters(text.into()))
  }
  pub fn write_cdata(&mut self, text: &str) -> Result<(), String> {
    self.write_event(XmlWriteEvent::CData(text.into()))
  }
}

/// Low-level xml-rs compatibility escape hatch.
impl<W: Write> Serializer<XmlRsWriter<W>> {
  pub fn new_from_writer(writer: W, config: &Config) -> Self {
    Self::new(XmlRsWriter::from_writer(writer, config))
  }
  pub fn new_for_inner(writer: W) -> Self {
    Self::new(XmlRsWriter::from_writer(
      writer,
      &Config {
        write_document_declaration: false,
        ..Config::default()
      },
    ))
  }
  pub fn write<'a, Event>(&mut self, event: Event) -> ::xml::writer::Result<()>
  where
    Event: Into<XmlEvent<'a>>,
  {
    self.writer.write(event)
  }
}
