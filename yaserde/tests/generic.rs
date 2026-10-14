#[macro_use]
extern crate yaserde;

use yaserde::{YaDeserialize, YaSerialize};

#[test]
fn serializer_writer_parameter_does_not_shadow_model_parameters() {
  #[derive(YaSerialize)]
  struct Generic<E: YaSerialize, __YaSerdeWriter: YaSerialize, __YaSerdeWriter_: YaSerialize> {
    first: E,
    second: __YaSerdeWriter,
    third: __YaSerdeWriter_,
  }

  let model = Generic {
    first: 1u32,
    second: 2u32,
    third: 3u32,
  };
  let content = "<Generic>123</Generic>";
  serialize_and_validate!(model, content);
}

fn init() {
  let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn generic() {
  init();

  #[derive(Debug, PartialEq, YaDeserialize, YaSerialize)]
  #[yaserde(rename = "base")]
  pub struct Base<E>
  where
    E: YaSerialize + YaDeserialize + Default,
  {
    background: E,
  }

  #[derive(Debug, Default, PartialEq, YaDeserialize, YaSerialize)]
  pub struct Generic {
    #[yaserde(attribute = true)]
    color: String,
  }

  let content = r#"<base><background color="blue" /></base>"#;
  let model = Base {
    background: Generic {
      color: "blue".to_string(),
    },
  };

  serialize_and_validate!(model, content);

  log::debug!("deserialize_and_validate @ {}:{}", file!(), line!());
  let loaded: Result<Base<Generic>, String> = yaserde::de::from_str(content);
  assert_eq!(loaded, Ok(model));
}
