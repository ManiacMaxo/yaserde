//! Generic data structure serialization framework.
//!

use crate::xml::{XmlAttribute, XmlName, XmlNamespace, XmlStartElement, XmlWriteEvent};
use crate::YaSerialize;
use ::xml::writer::XmlEvent;
use ::xml::{EmitterConfig, EventWriter};
use std::io::{Cursor, Write};
use std::str;

/// Serialize XML into a plain String with no formatting (EmitterConfig).
pub fn to_string<T: YaSerialize>(model: &T) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, &Config::default())?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

/// Serialize XML into a plain String with control on formatting (via EmitterConfig parameters)
pub fn to_string_with_config<T: YaSerialize>(model: &T, config: &Config) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer(model, buf, config)?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

pub fn serialize_with_writer<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
  config: &Config,
) -> Result<W, String> {
  let mut serializer = Serializer::new_from_writer(writer, config);
  match YaSerialize::serialize(model, &mut serializer) {
    Ok(()) => Ok(serializer.into_inner()),
    Err(msg) => Err(msg),
  }
}

pub fn to_string_content<T: YaSerialize>(model: &T) -> Result<String, String> {
  let buf = Cursor::new(Vec::new());
  let cursor = serialize_with_writer_content(model, buf)?;
  let data = str::from_utf8(cursor.get_ref()).expect("Found invalid UTF-8");
  Ok(data.into())
}

pub fn serialize_with_writer_content<W: Write, T: YaSerialize>(
  model: &T,
  writer: W,
) -> Result<W, String> {
  let mut serializer = Serializer::new_for_inner(writer);
  serializer.set_skip_start_end(true);
  match YaSerialize::serialize(model, &mut serializer) {
    Ok(()) => Ok(serializer.into_inner()),
    Err(msg) => Err(msg),
  }
}

pub struct Serializer<W: Write> {
  writer: EventWriter<W>,
  skip_start_end: bool,
  start_event_name: Option<String>,
  start_element_updater: Option<Box<dyn FnMut(&mut XmlStartElement) + Send + Sync + 'static>>,
}

impl<W: Write> Serializer<W> {
  pub fn new(writer: EventWriter<W>) -> Self {
    Serializer {
      writer,
      skip_start_end: false,
      start_event_name: None,
      start_element_updater: None,
    }
  }

  pub fn new_from_writer(writer: W, config: &Config) -> Self {
    let mut emitter_config = EmitterConfig::new()
      .cdata_to_characters(false)
      .perform_indent(config.perform_indent)
      .write_document_declaration(config.write_document_declaration);

    if let Some(indent_string_value) = &config.indent_string {
      emitter_config = emitter_config.indent_string(indent_string_value.clone());
    }

    Self::new(EventWriter::new_with_config(writer, emitter_config))
  }

  pub fn new_for_inner(writer: W) -> Self {
    let config = EmitterConfig::new().write_document_declaration(false);

    Self::new(EventWriter::new_with_config(writer, config))
  }

  pub fn into_inner(self) -> W {
    self.writer.into_inner()
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

  /// Sets a per-serializer callback invoked once for every start element.
  ///
  /// The callback must be `Send + Sync + 'static`; use `move` to capture owned configuration.
  pub fn set_start_element_updater<F>(&mut self, updater: F)
  where
    F: FnMut(&mut XmlStartElement) + Send + Sync + 'static,
  {
    self.start_element_updater = Some(Box::new(updater));
  }

  pub fn clear_start_element_updater(&mut self) {
    self.start_element_updater = None;
  }

  pub fn write<'a, E>(&mut self, event: E) -> ::xml::writer::Result<()>
  where
    E: Into<XmlEvent<'a>>,
  {
    let event = event.into();
    if self.start_element_updater.is_none() {
      return self.writer.write(event);
    }
    match event {
      XmlEvent::StartElement {
        name,
        attributes,
        namespace,
      } => self.write_updated_start_element(XmlStartElement::new(
        name.to_owned().into(),
        attributes
          .into_owned()
          .into_iter()
          .map(|attribute| {
            XmlAttribute::new(attribute.name.to_owned().into(), attribute.value.to_owned())
          })
          .collect(),
        namespace.into_owned().into(),
      )),
      event => self.writer.write(event),
    }
  }

  pub fn write_event(&mut self, event: XmlWriteEvent<'_>) -> Result<(), String> {
    match event {
      XmlWriteEvent::StartElement {
        name,
        attributes,
        namespace,
      } => self.write_start_element(name.into_owned(), attributes, namespace),
      XmlWriteEvent::EndElement => self.write_end_element(),
      XmlWriteEvent::Characters(text) => self.write_characters(&text),
      XmlWriteEvent::CData(text) => self.write_cdata(&text),
    }
  }

  pub fn write_start_element<S: Into<String>>(
    &mut self,
    name: S,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> Result<(), String> {
    let name = name.into();
    if self.start_element_updater.is_none() {
      return self
        .write_start_element_unmodified(name, attributes, namespace)
        .map_err(|e| e.to_string());
    }
    self
      .write_updated_start_element(Self::start_element_from_name(name, attributes, namespace))
      .map_err(|e| e.to_string())
  }

  fn start_element_from_name(
    name: String,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> XmlStartElement {
    let name = match name.split_once(':') {
      Some((prefix, local_name)) => XmlName {
        local_name: local_name.to_owned(),
        namespace: namespace.0.get(prefix).cloned(),
        prefix: Some(prefix.to_owned()),
      },
      None => XmlName {
        local_name: name,
        namespace: namespace.0.get("").cloned(),
        prefix: None,
      },
    };
    XmlStartElement::new(name, attributes, namespace)
  }

  fn write_updated_start_element(
    &mut self,
    mut event: XmlStartElement,
  ) -> ::xml::writer::Result<()> {
    if let Some(updater) = self.start_element_updater.as_mut() {
      updater(&mut event);
    }
    self.write_start_element_owned(event.name, event.attributes, event.namespace)
  }

  fn write_start_element_unmodified(
    &mut self,
    name: String,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> ::xml::writer::Result<()> {
    let name = ::xml::name::OwnedName::local(name);
    self.write_start_element_xml_rs(name, attributes, namespace)
  }

  fn write_start_element_owned(
    &mut self,
    name: XmlName,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> ::xml::writer::Result<()> {
    self.write_start_element_xml_rs(name.to_xml_rs(), attributes, namespace)
  }

  fn write_start_element_xml_rs(
    &mut self,
    name: ::xml::name::OwnedName,
    attributes: Vec<XmlAttribute>,
    namespace: XmlNamespace,
  ) -> ::xml::writer::Result<()> {
    let attributes: Vec<_> = attributes
      .iter()
      .map(|attribute| attribute.to_xml_rs())
      .collect();
    let attributes = attributes
      .iter()
      .map(|attribute| attribute.borrow())
      .collect();
    self
      .writer
      .write(::xml::writer::events::XmlEvent::StartElement {
        name: name.borrow(),
        attributes: ::std::borrow::Cow::Owned(attributes),
        namespace: ::std::borrow::Cow::Owned(namespace.to_xml_rs()),
      })
  }

  pub fn write_end_element(&mut self) -> Result<(), String> {
    self
      .writer
      .write(::xml::writer::XmlEvent::end_element())
      .map_err(|e| e.to_string())
  }

  pub fn write_characters(&mut self, text: &str) -> Result<(), String> {
    self
      .writer
      .write(::xml::writer::XmlEvent::characters(text))
      .map_err(|e| e.to_string())
  }

  pub fn write_cdata(&mut self, text: &str) -> Result<(), String> {
    self
      .writer
      .write(::xml::writer::events::XmlEvent::cdata(text))
      .map_err(|e| e.to_string())
  }
}

pub struct Config {
  pub perform_indent: bool,
  pub write_document_declaration: bool,
  pub indent_string: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Config {
      perform_indent: false,
      write_document_declaration: true,
      indent_string: None,
    }
  }
}
