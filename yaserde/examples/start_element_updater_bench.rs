use std::env;
use std::hint::black_box;
use std::io::Cursor;
use std::time::Instant;
use yaserde::ser::Serializer;
use yaserde::xml::{XmlEventReader, XmlReadEvent, XmlRsReader};
use yaserde::YaSerialize;
#[cfg(not(feature = "derive"))]
use yaserde_derive::YaSerialize;

const CWMP_10: &str = "urn:dslforum-org:cwmp-1-0";
const CWMP_13: &str = "urn:dslforum-org:cwmp-1-3";
const SOAP_ENV: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const SOAP_ENC: &str = "http://schemas.xmlsoap.org/soap/encoding/";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const XSD: &str = "http://www.w3.org/2001/XMLSchema";

#[derive(YaSerialize)]
#[yaserde(prefix = "soapenv", namespaces = { "soapenv" = "http://schemas.xmlsoap.org/soap/envelope/", "soapenc" = "http://schemas.xmlsoap.org/soap/encoding/", "xsi" = "http://www.w3.org/2001/XMLSchema-instance", "xsd" = "http://www.w3.org/2001/XMLSchema", "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct Envelope {
  #[yaserde(rename = "Header", prefix = "soapenv")]
  header: Header,
  #[yaserde(rename = "Body", prefix = "soapenv")]
  body: Body,
}

#[derive(YaSerialize)]
#[yaserde(namespaces = { "cwmp" = "urn:dslforum-org:cwmp-1-0" })]
struct Header {
  #[yaserde(rename = "ID", prefix = "cwmp")]
  id: String,
}

#[derive(YaSerialize)]
#[yaserde(prefix = "soapenv", namespaces = { "soapenv" = "http://schemas.xmlsoap.org/soap/envelope/" })]
struct Body {
  #[yaserde(rename = "GetParameterValuesResponse", prefix = "cwmp")]
  response: GetParameterValuesResponse,
}

#[derive(YaSerialize)]
#[yaserde(prefix = "cwmp", namespaces = { "cwmp" = "urn:dslforum-org:cwmp-1-0", "soapenv" = "http://schemas.xmlsoap.org/soap/envelope/", "soapenc" = "http://schemas.xmlsoap.org/soap/encoding/", "xsi" = "http://www.w3.org/2001/XMLSchema-instance", "xsd" = "http://www.w3.org/2001/XMLSchema" })]
struct GetParameterValuesResponse {
  #[yaserde(rename = "ParameterList")]
  parameters: ParameterValueList,
}

#[derive(YaSerialize)]
struct ParameterValueList {
  #[yaserde(attribute = true, prefix = "soapenc", rename = "arrayType")]
  array_type: Option<String>,
  #[yaserde(rename = "ParameterValueStruct")]
  entries: Vec<ParameterValue>,
}

#[derive(YaSerialize)]
struct ParameterValue {
  #[yaserde(rename = "Name")]
  name: String,
  #[yaserde(rename = "Value")]
  value: Option<Value>,
}

#[derive(YaSerialize)]
struct Value {
  #[yaserde(attribute = true, prefix = "xsi", rename = "type")]
  xsi_type: Option<String>,
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
      Self::Update => "cwmp 1-0 -> 1-3",
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

fn payload(size: usize) -> Envelope {
  Envelope {
    header: Header {
      id: "codex-request-42".to_owned(),
    },
    body: Body {
      response: GetParameterValuesResponse {
        parameters: ParameterValueList {
          array_type: Some(format!("cwmp:ParameterValueStruct[{size}]")),
          entries: (0..size)
            .map(|index| ParameterValue {
              name: format!("Device.WiFi.SSID.{index}.SSID"),
              value: Some(Value {
                xsi_type: Some("xsd:string".to_owned()),
                content: Some(format!("codex-value-{index}")),
              }),
            })
            .collect(),
        },
      },
    },
  }
}

fn serialize(payload: &Envelope, case: Case) -> String {
  let mut serializer = Serializer::new_for_inner(Cursor::new(Vec::new()));
  match case {
    Case::NoHook => {}
    Case::Noop => serializer.set_start_element_updater(|_| {}),
    Case::Update => serializer.set_start_element_updater(|event| {
      event.update_namespace("cwmp", CWMP_13);
    }),
  }
  payload.serialize(&mut serializer).unwrap();
  String::from_utf8(serializer.into_inner().into_inner()).unwrap()
}

fn assert_cwmp_namespace(xml: &str) {
  let mut reader = XmlRsReader::from_reader(xml.as_bytes());
  loop {
    match reader.next_event().unwrap() {
      XmlReadEvent::StartElement { name, .. } if name.prefix_ref() == Some("cwmp") => {
        assert_eq!(name.namespace_ref(), Some(CWMP_13));
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
  assert!(updated.contains(CWMP_13));
  assert!(!updated.contains(CWMP_10));
  assert_cwmp_namespace(&updated);
  assert!(updated.contains(SOAP_ENV));
  assert!(updated.contains(SOAP_ENC));
  assert!(updated.contains(XSI));
  assert!(updated.contains(XSD));
  assert!(updated.contains(&format!(
    "soapenc:arrayType=\"cwmp:ParameterValueStruct[{size}]\""
  )));
  assert!(updated.contains("xsi:type=\"xsd:string\""));
  assert!(updated.contains("codex-request-42"));
  assert!(updated.contains("Device.WiFi.SSID.0.SSID"));
  assert!(updated.contains(&format!("codex-value-{}", size.saturating_sub(1))));

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
